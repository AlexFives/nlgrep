use std::io;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("query must not be empty or whitespace-only")]
    EmptyQuery,
    #[error("probability must be finite and within [0, 1], got {0}")]
    InvalidProbability(String),
    #[error("invalid batch plan: {0}")]
    InvalidBatchPlan(String),
    #[error("invalid batch response: {0}")]
    InvalidBatchResponse(String),
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter configuration error: {0}")]
    Configuration(String),
    #[error("authentication failed")]
    Authentication,
    #[error("adapter request timed out")]
    Timeout,
    #[error("transport failed after retry: {0}")]
    Transport(String),
    #[error("provider rejected the request with HTTP status {status}: {message}")]
    ProviderRejected { status: u16, message: String },
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("adapter operation was cancelled")]
    Cancellation,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("--all requires --json")]
    AllRequiresJson,
    #[error("invalid query: {0}")]
    InvalidQuery(#[from] DomainError),
    #[error("TYPESAFE_API_KEY is not set")]
    MissingApiKey,
    #[error("{0} is not set for the selected provider")]
    MissingProviderApiKey(String),
    #[error("API key must not be empty")]
    EmptyApiKey,
    #[error("model must not be empty")]
    EmptyModel,
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("invalid provider {provider}: {message}")]
    InvalidProvider {
        provider: String,
        message: &'static str,
    },
    #[error("could not read configuration {}: {source}", path.display())]
    ReadConfig {
        path: std::path::PathBuf,
        source: io::Error,
    },
    #[error("invalid TOML or unsupported configuration in {} at line {line}, column {column}", path.display())]
    ParseConfig {
        path: std::path::PathBuf,
        line: usize,
        column: usize,
    },
    #[error("could not build the HTTP client: {0}")]
    HttpClient(String),
}

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("could not write output: {0}")]
    Io(#[from] io::Error),
    #[error("could not encode JSON output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("input filename is not valid UTF-8: {path}")]
    NonUtf8Path { path: String },
}
