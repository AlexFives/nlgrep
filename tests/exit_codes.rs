use assert_cmd::Command;
use nlgrep::{
    AdapterError, AdapterFuture, BatchPlan, BatchRequest, BatchResponse, LogicalRequest,
    ModelAdapter, Probability, Query, RunConfig, run_with_adapter,
};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAdapter {
    calls: AtomicUsize,
}

impl CountingAdapter {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl ModelAdapter for CountingAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(
            std::iter::once(0..request.items.len()).collect(),
        ))
    }

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
        Box::pin(async move {
            let judgments = request
                .items
                .iter()
                .map(|item| {
                    let value = if item.text == "banana" { 0.8 } else { 0.2 };
                    let probability = Probability::try_from(value).expect("valid fixture value");
                    nlgrep::Judgment::new(item.id, probability)
                })
                .collect();
            BatchResponse::try_new(&ids, judgments)
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
        })
    }
}

fn config(json: bool, all: bool) -> RunConfig {
    RunConfig {
        query: Query::try_new("select fruit").expect("valid query"),
        threshold: Probability::try_from(0.5).expect("valid threshold"),
        json,
        all,
        model: "jev-latest".to_owned(),
        timeout: std::time::Duration::from_secs(30),
        concurrency: 4,
        files: Vec::new(),
    }
}

#[test]
fn missing_api_key_is_exit_two_and_does_not_print_the_key() {
    let mut command = Command::cargo_bin("nlgrep").expect("binary should be built");
    command
        .env_remove("TYPESAFE_API_KEY")
        .args(["select fruit"])
        .write_stdin("banana\n")
        .assert()
        .code(2)
        .stderr(predicates::str::contains("TYPESAFE_API_KEY"));
}

#[tokio::test]
async fn empty_input_is_exit_one_without_an_api_request() {
    let adapter = CountingAdapter::new();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false),
        &adapter,
        &mut input,
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(outcome.exit_code, 1);
    assert_eq!(adapter.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn one_match_is_exit_zero() {
    let adapter = CountingAdapter::new();
    let mut input = Cursor::new(b"car\nbanana\n".to_vec());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false),
        &adapter,
        &mut input,
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(outcome.exit_code, 0);
    assert_eq!(stdout, b"banana\n");
}

#[tokio::test]
async fn no_matches_is_exit_one() {
    let adapter = CountingAdapter::new();
    let mut input = Cursor::new(b"car\npotato\n".to_vec());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false),
        &adapter,
        &mut input,
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(outcome.exit_code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}
