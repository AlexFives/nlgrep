use super::*;
use crate::{
    AdapterFuture, Candidate, CandidateId, InputRecord, Judgment, LineEnding, LogicalRequest,
    Source, read_bytes,
};
use futures_util::stream;
use std::io;

#[derive(Default)]
struct Sink {
    batches: Vec<Vec<Decision>>,
}

impl DecisionSink for Sink {
    fn write(&mut self, decisions: &[Decision]) -> Result<(), OutputError> {
        self.batches.push(decisions.to_vec());
        Ok(())
    }
}

struct InvalidPlanAdapter;

impl ModelAdapter for InvalidPlanAdapter {
    fn plan_batches(&self, _request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(std::iter::once(0..0).collect()))
    }

    fn classify_batch<'a>(
        &'a self,
        _request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async { Err(AdapterError::Transport("must not classify".to_owned())) })
    }
}

struct FailingPlanAdapter;

impl ModelAdapter for FailingPlanAdapter {
    fn plan_batches(&self, _request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Err(AdapterError::Transport("fixture plan failure".to_owned()))
    }

    fn classify_batch<'a>(
        &'a self,
        _request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async { Err(AdapterError::Transport("must not classify".to_owned())) })
    }
}

struct ResponseAdapter {
    unknown_candidate: bool,
}

impl ModelAdapter for ResponseAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(
            std::iter::once(0..request.items.len()).collect(),
        ))
    }

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async move {
            let judgments = if self.unknown_candidate {
                vec![Judgment::new(CandidateId::new(99), threshold())]
            } else {
                request
                    .items
                    .iter()
                    .map(|item| Judgment::new(item.id, threshold()))
                    .collect()
            };
            Ok(BatchResponse { judgments })
        })
    }
}

struct MultiBatchAdapter;

impl ModelAdapter for MultiBatchAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(vec![0..1, 1..request.items.len()]))
    }

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async move {
            Ok(BatchResponse {
                judgments: request
                    .items
                    .iter()
                    .map(|item| Judgment::new(item.id, threshold()))
                    .collect(),
            })
        })
    }
}

struct FailingBatchAdapter;

impl ModelAdapter for FailingBatchAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(
            std::iter::once(0..request.items.len()).collect(),
        ))
    }

    fn classify_batch<'a>(
        &'a self,
        _request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async { Err(AdapterError::Transport("fixture batch failure".to_owned())) })
    }
}

struct FailingSink;

impl DecisionSink for FailingSink {
    fn write(&mut self, _decisions: &[Decision]) -> Result<(), OutputError> {
        Err(OutputError::Io(io::Error::other("fixture sink failure")))
    }
}

fn records() -> Vec<InputRecord> {
    read_bytes(b"banana\ncar").expect("fixture input should be valid")
}

fn threshold() -> Probability {
    Probability::try_from(0.5).expect("fixture threshold should be valid")
}

#[tokio::test]
async fn empty_records_short_circuit_without_planning() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let summary = run_classification(
        &ResponseAdapter {
            unknown_candidate: false,
        },
        &query,
        &[],
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect("empty input should be a successful no-match");

    assert!(!summary.matched);
}

#[tokio::test]
async fn empty_stream_with_pending_batch_returns_missing_batch() {
    let mut sink = Sink::default();
    let error = consume_results(
        Box::pin(stream::empty::<Result<(usize, BatchResponse), AdapterError>>()),
        &records(),
        threshold(),
        1,
        &mut sink,
    )
    .await
    .expect_err("missing batch should be rejected");

    assert!(matches!(
        error,
        OrchestratorError::MissingBatch { index: 0 }
    ));
}

#[test]
fn unknown_judgment_id_is_rejected_during_projection() {
    let records = records();
    let response = BatchResponse {
        judgments: vec![Judgment::new(CandidateId::new(99), threshold())],
    };

    let error = project_decisions(response, &records, threshold())
        .expect_err("unknown candidate must be rejected");

    assert!(matches!(
        error,
        OrchestratorError::UnknownCandidate { id: 99 }
    ));
}

#[test]
fn decision_projection_keeps_source_and_line_metadata() {
    let record = InputRecord {
        candidate: Candidate::new(CandidateId::new(4), "line"),
        source: Source::stdin(),
        line_number: 9,
        line_ending: LineEnding::CrLf,
    };
    let response = BatchResponse {
        judgments: vec![Judgment::new(CandidateId::new(4), threshold())],
    };

    let decisions =
        project_decisions(response, &[record], threshold()).expect("projection should succeed");

    assert_eq!(decisions[0].line_number, 9);
    assert_eq!(decisions[0].line_ending, LineEnding::CrLf);
    assert!(decisions[0].matched);
}

#[tokio::test]
async fn adapter_plan_errors_are_returned_by_classification() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let error = run_classification(
        &InvalidPlanAdapter,
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("invalid adapter plans should be rejected");

    assert!(matches!(error, OrchestratorError::Plan(_)));
}

#[tokio::test]
async fn adapter_failures_are_returned_by_classification() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let error = run_classification(
        &FailingPlanAdapter,
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("adapter planning failures should be returned");

    assert!(matches!(error, OrchestratorError::Adapter(_)));
}

#[tokio::test]
async fn projection_errors_are_returned_by_classification() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let error = run_classification(
        &ResponseAdapter {
            unknown_candidate: true,
        },
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("unknown adapter judgments should be rejected");

    assert!(matches!(
        error,
        OrchestratorError::UnknownCandidate { id: 99 }
    ));
}

#[tokio::test]
async fn batch_failures_are_returned_by_classification() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let error = run_classification(
        &FailingBatchAdapter,
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("batch failures should be returned");

    assert!(matches!(error, OrchestratorError::Adapter(_)));
}

#[tokio::test]
async fn sink_errors_are_returned_by_classification() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = FailingSink;

    let error = run_classification(
        &ResponseAdapter {
            unknown_candidate: false,
        },
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 1,
        },
        &mut sink,
    )
    .await
    .expect_err("sink errors should be returned");

    assert!(matches!(error, OrchestratorError::Output(_)));
}

#[tokio::test]
async fn zero_concurrency_handles_multiple_batches_in_order() {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let mut sink = Sink::default();

    let summary = run_classification(
        &MultiBatchAdapter,
        &query,
        &records(),
        &RunOptions {
            threshold: threshold(),
            concurrency: 0,
        },
        &mut sink,
    )
    .await
    .expect("multiple unbounded batches should succeed");

    assert!(summary.matched);
    assert_eq!(sink.batches.len(), 2);
    assert_eq!(sink.batches[0][0].line_number, 1);
    assert_eq!(sink.batches[1][0].line_number, 2);
}
