use crate::{ConfigError, RunConfig, TypeSafeConfig};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env::{self, VarError},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<SecretString>,
    pub api_key_env: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            endpoint: crate::adapter::typesafe::DEFAULT_ENDPOINT.to_owned(),
            model: "jev-latest".to_owned(),
            api_key: None,
            api_key_env: None,
        }
    }
}

impl ProviderConfig {
    pub fn key_variable(&self) -> &str {
        self.api_key_env.as_deref().unwrap_or("TYPESAFE_API_KEY")
    }

    fn validate(&self, name: &str) -> Result<(), ConfigError> {
        let invalid = |message| ConfigError::InvalidProvider {
            provider: name.to_owned(),
            message,
        };
        if name.trim().is_empty() {
            return Err(invalid("provider name must not be empty"));
        }
        let endpoint = reqwest::Url::parse(&self.endpoint)
            .map_err(|_| invalid("endpoint must be an absolute HTTP or HTTPS URL"))?;
        if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
            return Err(invalid("endpoint must be an absolute HTTP or HTTPS URL"));
        }
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            return Err(invalid(
                "endpoint must not contain credentials; use api_key instead",
            ));
        }
        if self.model.trim().is_empty() {
            return Err(invalid("model must not be empty"));
        }
        if self.api_key.is_some() && self.api_key_env.is_some() {
            return Err(invalid("set either api_key or api_key_env"));
        }
        if self
            .api_key
            .as_ref()
            .is_some_and(|key| key.expose_secret().trim().is_empty())
        {
            return Err(invalid("api_key must not be empty"));
        }
        if self
            .api_key_env
            .as_ref()
            .is_some_and(|variable| variable.is_empty() || variable.contains(['=', '\0']))
        {
            return Err(invalid(
                "api_key_env must be a nonempty environment variable name",
            ));
        }
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    default_provider: Option<String>,
    providers: BTreeMap<String, ProviderConfig>,
}

impl FileConfig {
    fn load(&mut self, path: &Path, required: bool) -> Result<(), ConfigError> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if !required && error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(ConfigError::ReadConfig {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let config: Self = toml::from_str(&contents).map_err(|error: toml::de::Error| {
            let offset = error.span().map_or(0, |span| span.start);
            let prefix = &contents[..offset];
            ConfigError::ParseConfig {
                path: path.to_owned(),
                line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
                column: prefix
                    .rsplit('\n')
                    .next()
                    .map_or(1, |line| line.chars().count() + 1),
            }
        })?;
        for (name, provider) in &config.providers {
            provider.validate(name)?;
        }
        if config.default_provider.is_some() {
            self.default_provider = config.default_provider;
        }
        self.providers.extend(config.providers);
        Ok(())
    }
}

pub(crate) fn resolve_provider(
    name: Option<&str>,
    config_path: Option<&Path>,
) -> Result<ProviderConfig, ConfigError> {
    let mut config = FileConfig::default();
    if let Some(path) = config_path {
        config.load(path, true)?;
    } else {
        if let Some(path) = global_config_path() {
            config.load(&path, false)?;
        }
        config.load(Path::new(".nlgrep.toml"), false)?;
    }
    let name = name
        .or(config.default_provider.as_deref())
        .unwrap_or("typesafe");
    match config.providers.remove(name) {
        Some(provider) => Ok(provider),
        None if name == "typesafe" => Ok(ProviderConfig::default()),
        None => Err(ConfigError::UnknownProvider(name.to_owned())),
    }
}

fn global_config_path() -> Option<PathBuf> {
    let variable = |name| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let directory = variable("XDG_CONFIG_HOME")
        .or_else(|| cfg!(windows).then(|| variable("APPDATA")).flatten())
        .or_else(|| {
            variable(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| home.join(".config"))
        })?;
    Some(directory.join("nlgrep/config.toml"))
}

pub(crate) fn typesafe_config_with_key(
    config: &RunConfig,
    api_key: Result<String, VarError>,
) -> Result<TypeSafeConfig, ConfigError> {
    let api_key = match &config.provider.api_key {
        Some(key) => key.clone(),
        None => SecretString::from(api_key.map_err(|_| match config.provider.key_variable() {
            "TYPESAFE_API_KEY" => ConfigError::MissingApiKey,
            variable => ConfigError::MissingProviderApiKey(variable.to_owned()),
        })?),
    };
    if api_key.expose_secret().trim().is_empty() {
        return Err(ConfigError::EmptyApiKey);
    }
    Ok(
        TypeSafeConfig::new(api_key, config.model.clone(), config.timeout)
            .with_endpoint(config.provider.endpoint.clone()),
    )
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
