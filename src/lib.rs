pub mod adapter;
pub mod app;
pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod input;
pub mod orchestrator;
pub mod output;

pub use adapter::{AdapterFuture, ModelAdapter, TypeSafeAdapter, TypeSafeConfig, build_adapter};
pub use app::{
    RunOutcome, collect_inputs, finish_empty_input, production_adapter, run_with_adapter,
    run_with_inputs,
};
pub use cli::{Cli, RunConfig};
pub use domain::{
    BatchPlan, BatchRequest, BatchResponse, Candidate, CandidateId, Judgment, LogicalRequest,
    Probability, Query,
};
pub use error::{AdapterError, ConfigError, DomainError, OutputError};
pub use input::{
    InputError, InputReadResult, InputRecord, LineEnding, Source, read_bytes, read_inputs,
};
pub use orchestrator::{OrchestratorError, RunOptions, RunSummary, run_classification};
pub use output::{Decision, DecisionSink, OutputMode, OutputWriter};
