use crate::{ConfigError, RunConfig, TypeSafeConfig};
use secrecy::SecretString;
use std::env::VarError;

pub(crate) fn typesafe_config_with_key(
    config: &RunConfig,
    api_key: Result<String, VarError>,
) -> Result<TypeSafeConfig, ConfigError> {
    let api_key = api_key.map_err(|_| ConfigError::MissingApiKey)?;
    Ok(TypeSafeConfig::new(
        SecretString::from(api_key),
        config.model.clone(),
        config.timeout,
    ))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
