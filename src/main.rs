use clap::{Parser, error::ErrorKind};
use nlgrep::{
    Cli, ConfigError, ModelAdapter, RunConfig, collect_inputs, finish_empty_input,
    production_adapter, run_with_inputs,
};
use std::process::ExitCode;
use std::{
    ffi::OsString,
    io::{self, Read, Write},
};

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::stdout();
    let mut diagnostics = io::stderr().lock();
    run_process(
        std::env::args_os().collect(),
        &mut input,
        &mut output,
        &mut diagnostics,
        production_adapter,
        build_runtime,
    )
}

fn build_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

type AdapterFactory = fn(&RunConfig) -> Result<Box<dyn ModelAdapter>, ConfigError>;
type RuntimeFactory = fn() -> io::Result<tokio::runtime::Runtime>;

fn run_process(
    args: Vec<OsString>,
    input: &mut dyn Read,
    output: &mut (dyn Write + Send),
    diagnostics: &mut dyn Write,
    build_adapter: AdapterFactory,
    build_runtime: RuntimeFactory,
) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 2,
            };
            let _ = error.print();
            return ExitCode::from(exit_code);
        }
    };
    let config = match RunConfig::try_from(cli) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(diagnostics, "{error}");
            return ExitCode::from(2);
        }
    };
    let inputs = collect_inputs(&config, input);
    if inputs.records.is_empty() {
        return ExitCode::from(finish_empty_input(&inputs, diagnostics).exit_code);
    }
    let adapter = match build_adapter(&config) {
        Ok(adapter) => adapter,
        Err(error) => {
            let _ = writeln!(diagnostics, "{error}");
            return ExitCode::from(2);
        }
    };
    let runtime = match build_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = writeln!(diagnostics, "could not start async runtime: {error}");
            return ExitCode::from(2);
        }
    };
    let outcome = runtime.block_on(run_with_inputs(
        &config,
        inputs,
        adapter.as_ref(),
        output,
        diagnostics,
    ));
    ExitCode::from(outcome.exit_code)
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
