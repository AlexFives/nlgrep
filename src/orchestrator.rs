use crate::{
    AdapterError, BatchPlan, BatchRequest, BatchResponse, Candidate, CandidateId, Decision,
    DecisionSink, DomainError, InputRecord, ModelAdapter, OutputError, Probability, Query,
};
use futures_util::stream::{self, FuturesUnordered, Stream, StreamExt};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub threshold: Probability,
    pub concurrency: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub matched: bool,
}

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Plan(#[from] DomainError),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error("adapter returned an unknown candidate ID {id}")]
    UnknownCandidate { id: u64 },
    #[error("adapter result for batch {index} was not emitted")]
    MissingBatch { index: usize },
}

type BatchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(usize, BatchResponse), AdapterError>> + Send + 'a>>;
type BatchStream<'a> =
    Pin<Box<dyn Stream<Item = Result<(usize, BatchResponse), AdapterError>> + Send + 'a>>;

pub async fn run_classification(
    adapter: &dyn ModelAdapter,
    query: &Query,
    records: &[InputRecord],
    options: &RunOptions,
    sink: &mut dyn DecisionSink,
) -> Result<RunSummary, OrchestratorError> {
    if records.is_empty() {
        return Ok(RunSummary { matched: false });
    }

    let candidates = records
        .iter()
        .map(|record| record.candidate.clone())
        .collect::<Vec<_>>();
    let request = crate::LogicalRequest {
        query,
        items: &candidates,
    };
    let plan = adapter.plan_batches(&request)?;
    plan.validate(candidates.len())?;
    let futures = make_batch_futures(adapter, query, &candidates, &plan);

    if options.concurrency == 0 {
        let stream: BatchStream<'_> = Box::pin(FuturesUnordered::from_iter(futures));
        consume_results(stream, records, options.threshold, plan.ranges.len(), sink).await
    } else {
        let stream: BatchStream<'_> =
            Box::pin(stream::iter(futures).buffer_unordered(options.concurrency));
        consume_results(stream, records, options.threshold, plan.ranges.len(), sink).await
    }
}

fn make_batch_futures<'a>(
    adapter: &'a dyn ModelAdapter,
    query: &'a Query,
    candidates: &'a [Candidate],
    plan: &'a BatchPlan,
) -> Vec<BatchFuture<'a>> {
    plan.ranges
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, range)| {
            let request = BatchRequest {
                query,
                items: &candidates[range],
            };
            Box::pin(async move {
                adapter
                    .classify_batch(&request)
                    .await
                    .map(|response| (index, response))
            }) as BatchFuture<'a>
        })
        .collect()
}

async fn consume_results(
    mut stream: BatchStream<'_>,
    records: &[InputRecord],
    threshold: Probability,
    batch_count: usize,
    sink: &mut dyn DecisionSink,
) -> Result<RunSummary, OrchestratorError> {
    let mut pending = BTreeMap::new();
    let mut next_batch = 0;
    let mut matched = false;

    while let Some(result) = stream.next().await {
        let (batch_index, response) = result?;
        let decisions = project_decisions(response, records, threshold)?;
        pending.insert(batch_index, decisions);
        while let Some(decisions) = pending.remove(&next_batch) {
            matched |= decisions.iter().any(|decision| decision.matched);
            sink.write(&decisions)?;
            next_batch += 1;
        }
    }

    if next_batch != batch_count {
        return Err(OrchestratorError::MissingBatch { index: next_batch });
    }
    Ok(RunSummary { matched })
}

fn project_decisions(
    response: BatchResponse,
    records: &[InputRecord],
    threshold: Probability,
) -> Result<Vec<Decision>, OrchestratorError> {
    let by_id = records
        .iter()
        .map(|record| (record.candidate.id, record))
        .collect::<HashMap<CandidateId, &InputRecord>>();
    response
        .judgments
        .into_iter()
        .map(|judgment| {
            let record = by_id
                .get(&judgment.id)
                .ok_or(OrchestratorError::UnknownCandidate {
                    id: judgment.id.value(),
                })?;
            Ok(Decision::new(
                record.source.clone(),
                record.line_number,
                record.candidate.text.clone(),
                record.line_ending,
                judgment.probability,
                judgment.probability >= threshold,
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "orchestrator_tests.rs"]
mod tests;
