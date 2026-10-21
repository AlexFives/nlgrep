use crate::{
    Probability, Query,
    config::{ProviderConfig, resolve_provider},
    error::ConfigError,
};
use clap::Parser;
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Parser, Clone)]
#[command(name = "nlgrep", version, about = "Natural-language grep")]
pub struct Cli {
    #[arg(short = 't', long, default_value = "0.5")]
    pub threshold: Probability,
    #[arg(short = 'j', long)]
    pub json: bool,
    #[arg(long)]
    pub all: bool,
    #[arg(long, help = "Override the selected provider's model")]
    pub model: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Select a configured TypeSafe provider"
    )]
    pub provider: Option<String>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Read this TOML file instead of automatic configuration"
    )]
    pub config: Option<PathBuf>,
    #[arg(long, default_value = "30s", value_parser = parse_duration)]
    pub timeout: Duration,
    #[arg(long, default_value_t = 4)]
    pub concurrency: usize,
    #[arg(value_name = "QUERY")]
    pub query: String,
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub query: Query,
    pub threshold: Probability,
    pub json: bool,
    pub all: bool,
    pub model: String,
    pub provider: ProviderConfig,
    pub timeout: Duration,
    pub concurrency: usize,
    pub files: Vec<PathBuf>,
}

impl TryFrom<Cli> for RunConfig {
    type Error = ConfigError;

    fn try_from(cli: Cli) -> Result<Self, Self::Error> {
        if cli.all && !cli.json {
            return Err(ConfigError::AllRequiresJson);
        }
        let query = Query::try_new(cli.query).map_err(ConfigError::InvalidQuery)?;
        let provider = resolve_provider(cli.provider.as_deref(), cli.config.as_deref())?;
        let model = cli.model.unwrap_or_else(|| provider.model.clone());
        if model.trim().is_empty() {
            return Err(ConfigError::EmptyModel);
        }
        Ok(Self {
            query,
            threshold: cli.threshold,
            json: cli.json,
            all: cli.all,
            model,
            provider,
            timeout: cli.timeout,
            concurrency: cli.concurrency,
            files: cli.files,
        })
    }
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    humantime::parse_duration(value).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
