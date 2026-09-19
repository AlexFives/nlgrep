use crate::{
    AdapterError, ConfigError, DecisionSink, InputReadResult, ModelAdapter, OutputWriter,
    RunConfig, RunOptions, adapter::build_adapter, config::typesafe_config_with_key,
    input::read_inputs, orchestrator::run_classification,
};
use std::{
    env,
    io::{Read, Write},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOutcome {
    pub exit_code: u8,
    pub matched: bool,
}

pub fn collect_inputs(config: &RunConfig, input: &mut dyn Read) -> InputReadResult {
    read_inputs(&config.files, input)
}

pub fn finish_empty_input(inputs: &InputReadResult, diagnostics: &mut dyn Write) -> RunOutcome {
    report_input_errors(inputs, diagnostics);
    RunOutcome {
        exit_code: if inputs.errors.is_empty() { 1 } else { 2 },
        matched: false,
    }
}

pub async fn run_with_adapter<R, W, E>(
    config: &RunConfig,
    adapter: &dyn ModelAdapter,
    input: &mut R,
    output: &mut W,
    diagnostics: &mut E,
) -> RunOutcome
where
    R: Read,
    W: Write + Send,
    E: Write,
{
    let inputs = collect_inputs(config, input);
    run_with_inputs(config, inputs, adapter, output, diagnostics).await
}

pub async fn run_with_inputs(
    config: &RunConfig,
    inputs: InputReadResult,
    adapter: &dyn ModelAdapter,
    output: &mut (dyn Write + Send),
    diagnostics: &mut dyn Write,
) -> RunOutcome {
    if inputs.records.is_empty() {
        return finish_empty_input(&inputs, diagnostics);
    }
    report_input_errors(&inputs, diagnostics);
    let input_error = !inputs.errors.is_empty();
    let multiple_files = config.files.len() > 1;
    let options = RunOptions {
        threshold: config.threshold,
        concurrency: config.concurrency,
    };

    let result = if config.json {
        let mut sink = OutputWriter::json(&mut *output, config.all);
        classify_and_report(adapter, config, inputs, options, &mut sink, diagnostics).await
    } else {
        let mut sink = OutputWriter::text(&mut *output, multiple_files);
        classify_and_report(adapter, config, inputs, options, &mut sink, diagnostics).await
    };
    match result {
        Ok(matched) => RunOutcome {
            exit_code: if input_error {
                2
            } else if matched {
                0
            } else {
                1
            },
            matched,
        },
        Err(()) => RunOutcome {
            exit_code: 2,
            matched: false,
        },
    }
}

pub fn production_adapter(config: &RunConfig) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    production_adapter_with_key(config, env::var("TYPESAFE_API_KEY"))
}

fn production_adapter_with_key(
    config: &RunConfig,
    api_key: Result<String, env::VarError>,
) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    let adapter_config = typesafe_config_with_key(config, api_key)?;
    build_production_adapter(adapter_config)
}

fn build_production_adapter(
    adapter_config: crate::TypeSafeConfig,
) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    build_production_adapter_with(adapter_config, build_adapter)
}

type AdapterFactory = fn(crate::TypeSafeConfig) -> Result<Box<dyn ModelAdapter>, AdapterError>;

fn build_production_adapter_with(
    adapter_config: crate::TypeSafeConfig,
    build: AdapterFactory,
) -> Result<Box<dyn ModelAdapter>, ConfigError> {
    build(adapter_config).map_err(|error| ConfigError::HttpClient(error.to_string()))
}

async fn classify_and_report(
    adapter: &dyn ModelAdapter,
    config: &RunConfig,
    inputs: InputReadResult,
    options: RunOptions,
    sink: &mut dyn DecisionSink,
    diagnostics: &mut dyn Write,
) -> Result<bool, ()> {
    run_classification(adapter, &config.query, &inputs.records, &options, sink)
        .await
        .map(|summary| summary.matched)
        .map_err(|error| {
            let _ = writeln!(diagnostics, "{error}");
        })
}

fn report_input_errors(inputs: &InputReadResult, diagnostics: &mut dyn Write) {
    for error in &inputs.errors {
        let _ = writeln!(diagnostics, "{error}");
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
