use super::*;
use crate::{Probability, Query, RunConfig};
use std::path::PathBuf;
use std::time::Duration;

fn config() -> RunConfig {
    RunConfig {
        query: Query::try_new("query").expect("query should be valid"),
        threshold: Probability::try_from(0.5).expect("threshold should be valid"),
        json: false,
        all: false,
        model: "jev-test".to_owned(),
        timeout: Duration::from_secs(3),
        concurrency: 1,
        files: vec![PathBuf::from("words.txt")],
    }
}

#[test]
fn config_builder_converts_an_api_key_into_typesafe_config() {
    typesafe_config_with_key(&config(), Ok("test-key".to_owned()))
        .expect("API key should build configuration");
}

#[test]
fn config_builder_maps_missing_api_key() {
    let result = typesafe_config_with_key(&config(), Err(std::env::VarError::NotPresent));

    assert!(matches!(result, Err(ConfigError::MissingApiKey)));
}
