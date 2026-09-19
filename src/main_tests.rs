use super::*;
use nlgrep::{
    AdapterError, AdapterFuture, BatchPlan, BatchRequest, BatchResponse, Judgment, LogicalRequest,
    Probability,
};
use std::io::Cursor;

struct MatchingAdapter;

impl ModelAdapter for MatchingAdapter {
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
            let ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
            let judgments = request
                .items
                .iter()
                .map(|item| {
                    Judgment::new(
                        item.id,
                        Probability::try_from(1.0).expect("valid probability"),
                    )
                })
                .collect();
            BatchResponse::try_new(&ids, judgments)
                .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
        })
    }
}

struct FailingReader;

impl std::io::Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("fixture read failure"))
    }
}

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn successful_adapter(_config: &RunConfig) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    Ok(Box::new(MatchingAdapter))
}

fn successful_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn unexpected_adapter(_config: &RunConfig) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    panic!("adapter must not be built");
}

fn unexpected_runtime() -> io::Result<tokio::runtime::Runtime> {
    panic!("runtime must not be built");
}

fn fixture_adapter_error(_config: &RunConfig) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    Err(ConfigError::HttpClient("fixture adapter error".to_owned()))
}

fn fixture_runtime_error() -> io::Result<tokio::runtime::Runtime> {
    Err(std::io::Error::other("fixture runtime error"))
}

#[test]
fn production_runtime_builder_creates_a_runtime() {
    let runtime = super::build_runtime().expect("production runtime should build");
    runtime.block_on(async {});
}

#[test]
fn main_entry_is_callable() {
    let _ = super::main();
}

#[test]
fn help_and_version_are_successful_without_running_the_pipeline() {
    for command in ["--help", "--version"] {
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        let code = run_process(
            args(&["nlgrep", command]),
            &mut input,
            &mut output,
            &mut diagnostics,
            unexpected_adapter,
            unexpected_runtime,
        );
        assert_eq!(code, ExitCode::SUCCESS);
    }
}

#[test]
fn invalid_arguments_return_two() {
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        unexpected_adapter,
        unexpected_runtime,
    );

    assert_eq!(code, ExitCode::from(2));
}

#[test]
fn invalid_config_returns_two() {
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "--all", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        unexpected_adapter,
        unexpected_runtime,
    );

    assert_eq!(code, ExitCode::from(2));
    assert!(String::from_utf8_lossy(&diagnostics).contains("--all requires --json"));
}

#[test]
fn empty_input_returns_one_without_building_adapter() {
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        unexpected_adapter,
        unexpected_runtime,
    );

    assert_eq!(code, ExitCode::from(1));
    assert!(diagnostics.is_empty());
}

#[test]
fn input_error_returns_two_before_building_adapter() {
    let mut input = FailingReader;
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        unexpected_adapter,
        unexpected_runtime,
    );

    assert_eq!(code, ExitCode::from(2));
    assert!(String::from_utf8_lossy(&diagnostics).contains("fixture read failure"));
}

#[test]
fn adapter_error_returns_two_and_reports_it() {
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        fixture_adapter_error,
        unexpected_runtime,
    );

    assert_eq!(code, ExitCode::from(2));
    assert!(String::from_utf8_lossy(&diagnostics).contains("fixture adapter error"));
}

#[test]
fn runtime_error_returns_two_and_reports_it() {
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        successful_adapter,
        fixture_runtime_error,
    );

    assert_eq!(code, ExitCode::from(2));
    assert!(String::from_utf8_lossy(&diagnostics).contains("fixture runtime error"));
}

#[test]
fn successful_run_writes_matches() {
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let code = run_process(
        args(&["nlgrep", "select fruit"]),
        &mut input,
        &mut output,
        &mut diagnostics,
        successful_adapter,
        successful_runtime,
    );

    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(output, b"banana\n");
    assert!(diagnostics.is_empty());
}
