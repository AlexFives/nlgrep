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
        provider: ProviderConfig::default(),
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

#[test]
fn config_loader_rejects_invalid_provider_settings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config.toml");
    for settings in [
        "endpoint = 'not-a-url'",
        "endpoint = 'file:///tmp/typesafe'",
        "endpoint = 'https://user:password@example.com/v1/systemone'",
        "model = ' '",
        "api_key = ' '",
        "api_key_env = ''",
        "api_key_env = 'INVALID=NAME'",
        "api_key = 'key'\napi_key_env = 'KEY'",
    ] {
        fs::write(&path, format!("[providers.custom]\n{settings}\n")).expect("write configuration");

        let result = FileConfig::default().load(&path, true);

        assert!(matches!(result, Err(ConfigError::InvalidProvider { .. })));
    }
}

#[test]
fn config_loader_rejects_unknown_fields_and_incorrect_types() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config.toml");
    for contents in [
        "default_provder = 'custom'",
        "default_provider = 1",
        "[providers.custom]\nendpont = 'https://example.com'",
        "[providers.custom]\nmodel = 1",
        "[providers.custom]\napi_key = 42",
    ] {
        fs::write(&path, contents).expect("write configuration");

        let result = FileConfig::default().load(&path, true);

        assert!(matches!(result, Err(ConfigError::ParseConfig { .. })));
    }
}

#[test]
fn config_builder_rejects_an_empty_environment_key() {
    let result = typesafe_config_with_key(&config(), Ok(" ".to_owned()));

    assert!(matches!(result, Err(ConfigError::EmptyApiKey)));
}

#[test]
fn missing_provider_key_reports_the_configured_variable() {
    let mut config = config();
    config.provider.api_key_env = Some("PRIVATE_TYPESAFE_KEY".to_owned());

    let result = typesafe_config_with_key(&config, Err(VarError::NotPresent));

    assert!(
        matches!(result, Err(ConfigError::MissingProviderApiKey(variable)) if variable == "PRIVATE_TYPESAFE_KEY")
    );
}
