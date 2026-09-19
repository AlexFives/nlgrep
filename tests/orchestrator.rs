use nlgrep::{
    AdapterError, AdapterFuture, BatchPlan, BatchRequest, BatchResponse, Decision, DecisionSink,
    InputRecord, ModelAdapter, Probability, Query, RunOptions, Source, read_bytes,
    run_classification,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Barrier, Notify};
use tokio::time::{Duration, timeout};

struct FakeAdapter {
    batch_size: usize,
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    started: Option<Arc<Barrier>>,
    releases: Vec<Arc<Notify>>,
    fail_batch: Option<usize>,
    invalid_plan: bool,
}

impl FakeAdapter {
    fn new(batch_size: usize) -> Self {
        Self {
            batch_size,
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            started: None,
            releases: Vec::new(),
            fail_batch: None,
            invalid_plan: false,
        }
    }

    fn with_barrier(mut self, barrier: Arc<Barrier>) -> Self {
        self.started = Some(barrier);
        self
    }

    fn with_releases(mut self, releases: Vec<Arc<Notify>>) -> Self {
        self.releases = releases;
        self
    }

    fn with_failure(mut self, batch: usize) -> Self {
        self.fail_batch = Some(batch);
        self
    }

    fn with_invalid_plan(mut self) -> Self {
        self.invalid_plan = true;
        self
    }

    fn update_max_active(&self, current: usize) {
        let mut previous = self.max_active.load(Ordering::Relaxed);
        while current > previous {
            match self.max_active.compare_exchange_weak(
                previous,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => previous = observed,
            }
        }
    }
}

impl ModelAdapter for FakeAdapter {
    fn plan_batches(
        &self,
        request: &nlgrep::LogicalRequest<'_>,
    ) -> Result<BatchPlan, AdapterError> {
        if self.invalid_plan {
            return Ok(BatchPlan::new(vec![0..3, 2..request.items.len()]));
        }
        let ranges = (0..request.items.len())
            .step_by(self.batch_size)
            .map(|start| start..(start + self.batch_size).min(request.items.len()))
            .collect();
        Ok(BatchPlan::new(ranges))
    }

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        let batch_index = request
            .items
            .first()
            .map(|item| item.id.value() as usize / self.batch_size)
            .unwrap_or(0);
        let ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
        let started = self.started.clone();
        let release = self.releases.get(batch_index).cloned();
        let fail = self.fail_batch == Some(batch_index);
        let active_counter = &self.active;
        self.calls.fetch_add(1, Ordering::Relaxed);
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.update_max_active(active);

        Box::pin(async move {
            if let Some(barrier) = started {
                barrier.wait().await;
            }
            if let Some(notify) = release {
                notify.notified().await;
            }
            let result = if fail {
                Err(AdapterError::Transport("fake failure".to_owned()))
            } else {
                let judgments = ids
                    .iter()
                    .map(|id| {
                        let value = if id.value() == 0 { 0.5 } else { 0.8 };
                        let probability =
                            Probability::try_from(value).expect("fake probability is valid");
                        nlgrep::Judgment::new(*id, probability)
                    })
                    .collect();
                BatchResponse::try_new(&ids, judgments)
                    .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
            };
            active_counter.fetch_sub(1, Ordering::AcqRel);
            result
        })
    }
}

#[derive(Default)]
struct RecordingSink {
    batches: Vec<Vec<Decision>>,
}

impl DecisionSink for RecordingSink {
    fn write(&mut self, decisions: &[Decision]) -> Result<(), nlgrep::OutputError> {
        self.batches.push(decisions.to_vec());
        Ok(())
    }
}

fn fixture_records(count: usize) -> Vec<InputRecord> {
    let input = (0..count)
        .map(|index| format!("item-{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    read_bytes(input.as_bytes()).expect("fixture is valid UTF-8")
}

fn threshold() -> Probability {
    Probability::try_from(0.5).expect("fixture threshold is valid")
}

#[tokio::test]
async fn applies_threshold_and_emits_batches_in_input_order() {
    let releases = (0..3).map(|_| Arc::new(Notify::new())).collect::<Vec<_>>();
    let started = Arc::new(Barrier::new(4));
    let adapter = Arc::new(
        FakeAdapter::new(2)
            .with_barrier(started.clone())
            .with_releases(releases.clone()),
    );
    let query = Query::try_new("select fruit").expect("valid query");
    let records = fixture_records(6);
    let task = tokio::spawn({
        let adapter = adapter.clone();
        async move {
            let mut sink = RecordingSink::default();
            let summary = run_classification(
                adapter.as_ref(),
                &query,
                &records,
                &RunOptions {
                    threshold: threshold(),
                    concurrency: 0,
                },
                &mut sink,
            )
            .await;
            (summary, sink)
        }
    });

    started.wait().await;
    releases[2].notify_one();
    releases[1].notify_one();
    releases[0].notify_one();
    let (summary, sink) = task.await.expect("classification task completes");
    let summary = summary.expect("classification succeeds");

    assert!(summary.matched);
    assert_eq!(sink.batches.len(), 3);
    assert_eq!(sink.batches[0][0].source, Source::stdin());
    assert_eq!(sink.batches[0][0].line_number, 1);
    assert!(sink.batches[0][0].matched);
    assert_eq!(sink.batches[1][0].line_number, 3);
    assert_eq!(sink.batches[2][0].line_number, 5);
}

#[tokio::test]
async fn never_exceeds_requested_concurrency() {
    let adapter = FakeAdapter::new(2).with_barrier(Arc::new(Barrier::new(2)));
    let records = fixture_records(8);
    let query = Query::try_new("query").expect("valid query");
    let mut sink = RecordingSink::default();

    run_classification(
        &adapter,
        &query,
        &records,
        &RunOptions {
            threshold: threshold(),
            concurrency: 2,
        },
        &mut sink,
    )
    .await
    .expect("classification succeeds");

    assert!(adapter.max_active.load(Ordering::Relaxed) <= 2);
}

#[tokio::test]
async fn zero_concurrency_starts_all_planned_batches_without_a_local_cap() {
    let started = Arc::new(Barrier::new(4));
    let adapter = FakeAdapter::new(2).with_barrier(started.clone());
    let records = fixture_records(6);
    let query = Query::try_new("query").expect("valid query");
    let mut sink = RecordingSink::default();
    let task = tokio::spawn(async move {
        run_classification(
            &adapter,
            &query,
            &records,
            &RunOptions {
                threshold: threshold(),
                concurrency: 0,
            },
            &mut sink,
        )
        .await
    });

    if timeout(Duration::from_secs(1), started.wait())
        .await
        .is_err()
    {
        task.abort();
        panic!("all planned batches should start when concurrency is zero");
    }
    let summary = timeout(Duration::from_secs(1), task)
        .await
        .expect("classification task completes")
        .expect("classification task completes")
        .expect("success");
    assert!(summary.matched);
}

#[tokio::test]
async fn empty_input_skips_adapter_and_returns_no_matches() {
    let adapter = FakeAdapter::new(2);
    let query = Query::try_new("query").expect("valid query");
    let mut sink = RecordingSink::default();

    let summary = run_classification(
        &adapter,
        &query,
        &[],
        &RunOptions {
            threshold: threshold(),
            concurrency: 4,
        },
        &mut sink,
    )
    .await
    .expect("empty input succeeds");

    assert!(!summary.matched);
    assert_eq!(adapter.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn adapter_failure_returns_error_after_prior_ordered_output() {
    let adapter = FakeAdapter::new(2).with_failure(1);
    let records = fixture_records(6);
    let query = Query::try_new("query").expect("valid query");
    let mut sink = RecordingSink::default();

    let error = run_classification(
        &adapter,
        &query,
        &records,
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("adapter failure is returned");

    assert!(matches!(error, nlgrep::OrchestratorError::Adapter(_)));
    assert_eq!(sink.batches.len(), 1);
}

#[tokio::test]
async fn invalid_adapter_plan_is_rejected_before_classification() {
    let adapter = FakeAdapter::new(2).with_invalid_plan();
    let records = fixture_records(4);
    let query = Query::try_new("query").expect("valid query");
    let mut sink = RecordingSink::default();

    let error = run_classification(
        &adapter,
        &query,
        &records,
        &RunOptions {
            threshold: threshold(),
            concurrency: 2,
        },
        &mut sink,
    )
    .await
    .expect_err("invalid plan is rejected");

    assert!(matches!(error, nlgrep::OrchestratorError::Plan(_)));
    assert_eq!(adapter.calls.load(Ordering::Relaxed), 0);
}
