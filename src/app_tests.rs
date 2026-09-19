use super::*;
use crate::{
    AdapterError, AdapterFuture, BatchPlan, BatchRequest, BatchResponse, InputError,
    LogicalRequest, Probability, Query, read_bytes,
};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct FailingAdapter {
    calls: AtomicUsize,
}

impl FailingAdapter {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl ModelAdapter for FailingAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        Ok(BatchPlan::new(
            std::iter::once(0..request.items.len()).collect(),
        ))
    }

    fn classify_batch<'a>(
        &'a self,
        _request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(AdapterError::Transport("fixture failure".to_owned())) })
    }
}

struct FixedAdapter {
    probability: f64,
}

fn failing_factory(_: crate::TypeSafeConfig) -> Result<Box<dyn ModelAdapter>, AdapterError> {
    Err(AdapterError::Configuration("fixture failure".to_owned()))
}

impl ModelAdapter for FixedAdapter {
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
            let probability = Probability::try_from(self.probability)
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))?;
            let judgments = request
                .items
                .iter()
                .map(|item| crate::Judgment::new(item.id, probability))
                .collect();
            let ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
            BatchResponse::try_new(&ids, judgments)
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
        })
    }
}

fn config(json: bool) -> RunConfig {
    RunConfig {
        query: Query::try_new("query").expect("query should be valid"),
        threshold: Probability::try_from(0.5).expect("threshold should be valid"),
        json,
        all: json,
        model: "jev-latest".to_owned(),
        timeout: Duration::from_secs(30),
        concurrency: 1,
        files: Vec::new(),
    }
}

#[test]
fn empty_input_with_an_error_returns_exit_two_and_reports_it() {
    let inputs = InputReadResult {
        records: Vec::new(),
        errors: vec![InputError::RepeatedStdin],
    };
    let mut diagnostics = Vec::new();

    let outcome = finish_empty_input(&inputs, &mut diagnostics);

    assert_eq!(outcome.exit_code, 2);
    assert!(
        String::from_utf8(diagnostics)
            .expect("diagnostics should be UTF-8")
            .contains("stdin was requested more than once")
    );
}

#[tokio::test]
async fn adapter_failure_is_reported_as_exit_two() {
    let adapter = FailingAdapter::new();
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(false),
        &adapter,
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 2);
    assert!(output.is_empty());
    assert!(
        String::from_utf8(diagnostics)
            .expect("diagnostics should be UTF-8")
            .contains("transport failed after retry")
    );
    assert_eq!(adapter.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn empty_inputs_are_handled_by_the_async_entrypoint() {
    let adapter = FixedAdapter { probability: 0.8 };
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_inputs(
        &config(false),
        InputReadResult {
            records: Vec::new(),
            errors: Vec::new(),
        },
        &adapter,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 1);
    assert!(output.is_empty());
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn successful_json_matches_are_reported_with_exit_zero() {
    let adapter = FixedAdapter { probability: 0.8 };
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(true),
        &adapter,
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 0);
    assert!(
        String::from_utf8(output)
            .expect("JSON output should be UTF-8")
            .contains("\"matched\":true")
    );
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn successful_text_without_matches_returns_exit_one() {
    let adapter = FixedAdapter { probability: 0.1 };
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(false),
        &adapter,
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 1);
    assert!(!outcome.matched);
    assert!(output.is_empty());
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn nonempty_input_errors_override_a_successful_classification() {
    let adapter = FixedAdapter { probability: 0.8 };
    let inputs = InputReadResult {
        records: read_bytes(b"banana\n").expect("fixture input should be valid"),
        errors: vec![InputError::RepeatedStdin],
    };
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_inputs(
        &config(false),
        inputs,
        &adapter,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 2);
    assert!(outcome.matched);
    assert!(String::from_utf8_lossy(&diagnostics).contains("stdin was requested more than once"));
}

#[test]
fn production_adapter_maps_a_factory_error_boundary() {
    let config = crate::TypeSafeConfig::new(
        secrecy::SecretString::from("test-key"),
        "jev-latest".to_owned(),
        Duration::from_secs(1),
    );

    let adapter = build_production_adapter(config).expect("factory should build adapter");

    let query = Query::try_new("query").expect("query should be valid");
    let request = LogicalRequest {
        query: &query,
        items: &[],
    };
    assert!(adapter.plan_batches(&request).is_ok());
}

#[test]
fn production_adapter_maps_factory_failures_to_configuration_errors() {
    let config = crate::TypeSafeConfig::new(
        secrecy::SecretString::from("test-key"),
        "jev-latest".to_owned(),
        Duration::from_secs(1),
    );

    let result = build_production_adapter_with(config, failing_factory);

    assert!(
        matches!(result, Err(ConfigError::HttpClient(message)) if message == "adapter configuration error: fixture failure")
    );
}

#[test]
fn production_adapter_reads_configuration_before_building_the_factory() {
    let result = production_adapter_with_key(&config(false), Ok("test-key".to_owned()));

    assert!(result.is_ok());
}

#[test]
fn production_adapter_propagates_a_missing_api_key() {
    let result = production_adapter_with_key(&config(false), Err(std::env::VarError::NotPresent));

    assert!(matches!(result, Err(ConfigError::MissingApiKey)));
}
