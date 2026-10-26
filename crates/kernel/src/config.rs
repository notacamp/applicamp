//! Configuration from the environment (`APPLICAMP_…`), with `figment`.
//!
//! Nested settings use `__`: `APPLICAMP_OIDC__CAMPBOOKS__CLIENT_ID` is
//! `oidc.campbooks.client_id`. A secret can be given inline (`…_SECRET`) or,
//! preferably, as a file (`…_SECRET_FILE`), which wins when both are set.

use std::path::{Path, PathBuf};

use figment::{Figment, providers::Env};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

/// Prefix of every setting.
pub const ENV_PREFIX: &str = "APPLICAMP_";
/// Separator of nested settings.
pub const ENV_SEPARATOR: &str = "__";

/// Where every setting is read from: the `APPLICAMP_` variables.
pub fn env_source() -> Figment {
    Figment::from(Env::prefixed(ENV_PREFIX).split(ENV_SEPARATOR))
}

/// A setting is missing or malformed. The message names the variable,
/// never a secret's value.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// One JSON object per line; for production and log shippers.
    #[default]
    Json,
    /// Human-readable output; for local development.
    Pretty,
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Postgres connection string (`APPLICAMP_DATABASE_URL`). Secret: it
    /// usually embeds a password.
    pub database_url: SecretString,
    /// Largest number of database connections (`APPLICAMP_DATABASE_MAX_CONNECTIONS`).
    #[serde(default = "default_max_connections")]
    pub database_max_connections: u32,
    /// Where the HTTP server listens (`APPLICAMP_BIND_ADDR`).
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    /// The URL people reach this instance at (`APPLICAMP_PUBLIC_URL`): the
    /// origin unsafe requests must come from, and the base of the sign-in
    /// callbacks.
    #[serde(default = "default_public_url")]
    pub public_url: String,
    /// `json` (default) or `pretty` (`APPLICAMP_LOG_FORMAT`).
    #[serde(default)]
    pub log_format: LogFormat,
    /// Apply database migrations on startup (`APPLICAMP_RUN_MIGRATIONS`,
    /// default `true`).
    #[serde(default = "yes")]
    pub run_migrations: bool,
    /// The instance secret, base64 (`APPLICAMP_SECRET_KEY`); prefer
    /// [`secret_key_file`](Self::secret_key_file).
    #[serde(default)]
    pub secret_key: Option<SecretString>,
    /// A file holding the instance secret (`APPLICAMP_SECRET_KEY_FILE`).
    #[serde(default)]
    pub secret_key_file: Option<PathBuf>,
    /// The sign-in providers (`APPLICAMP_OIDC__…`).
    #[serde(default)]
    pub oidc: OidcConfig,
    /// The built web client (`clients/app/dist`) to serve beside the API
    /// (`APPLICAMP_WEB_DIR`; the image sets `/app/web`). Unset, the server
    /// answers only the API, the BFF and `/healthz`.
    #[serde(default)]
    pub web_dir: Option<PathBuf>,
}

/// The sign-in providers. One with no client id is not offered.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OidcConfig {
    /// Sign in with Campbooks (`APPLICAMP_OIDC__CAMPBOOKS__…`).
    #[serde(default)]
    pub campbooks: ProviderConfig,
    /// Sign in with LinkedIn (`APPLICAMP_OIDC__LINKEDIN__…`).
    #[serde(default)]
    pub linkedin: ProviderConfig,
}

/// One OpenID Connect provider this instance is a confidential client of.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    /// The issuer; its discovery document is at
    /// `<issuer>/.well-known/openid-configuration`.
    #[serde(default)]
    pub issuer: Option<String>,
    /// The client id registered at the provider.
    #[serde(default)]
    pub client_id: Option<String>,
    /// The client secret, inline (development only).
    #[serde(default)]
    pub client_secret: Option<SecretString>,
    /// A file holding the client secret.
    #[serde(default)]
    pub client_secret_file: Option<PathBuf>,
}

fn default_max_connections() -> u32 {
    10
}

fn default_bind_addr() -> String {
    "0.0.0.0:8080".to_owned()
}

fn default_public_url() -> String {
    "http://localhost:8080".to_owned()
}

const fn yes() -> bool {
    true
}

impl Config {
    /// The configuration in the environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_figment(&env_source())
    }

    /// The configuration in `figment` (tests build one by hand).
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract()
            .map_err(|e| ConfigError(format!("invalid configuration: {e}")))
    }

    /// A configuration for `database_url` with every other setting at its
    /// default: tests and tools.
    pub fn for_database(database_url: &str) -> Self {
        Self {
            database_url: SecretString::from(database_url.to_owned()),
            database_max_connections: default_max_connections(),
            bind_addr: default_bind_addr(),
            public_url: default_public_url(),
            log_format: LogFormat::Json,
            run_migrations: true,
            secret_key: None,
            secret_key_file: None,
            oidc: OidcConfig::default(),
            web_dir: None,
        }
    }
}

/// The secret in `file` (preferred) or `inline`, trimmed; `name` is the
/// variable reported on failure. `Ok(None)` when neither is set.
pub fn read_secret(
    name: &str,
    inline: Option<&SecretString>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, ConfigError> {
    let value = match (file, inline) {
        (Some(path), _) => std::fs::read_to_string(path)
            .map_err(|e| ConfigError(format!("{name}_FILE={}: {e}", path.display())))?,
        (None, Some(secret)) => secret.expose_secret().to_owned(),
        (None, None) => return Ok(None),
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(ConfigError(format!("{name} is empty")));
    }
    Ok(Some(SecretString::from(value.to_owned())))
}

#[cfg(test)]
mod tests {
    use figment::providers::Serialized;

    use super::*;

    #[test]
    fn nested_provider_settings_are_read() {
        let figment = Figment::from(Serialized::defaults(serde_json::json!({
            "database_url": "postgres://localhost/x",
            "oidc": { "campbooks": {
                "issuer": "https://campbooks.example.com",
                "client_id": "applicamp",
            }},
        })));
        let config = Config::from_figment(&figment).unwrap();
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert!(config.run_migrations);
        assert_eq!(
            config.oidc.campbooks.client_id.as_deref(),
            Some("applicamp")
        );
        assert!(config.oidc.linkedin.client_id.is_none());
    }

    #[test]
    fn a_secret_file_wins_and_is_trimmed() {
        let dir = std::env::temp_dir().join(format!("applicamp-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("secret");
        std::fs::write(&file, "from-file\n").unwrap();
        let inline = SecretString::from("inline".to_owned());
        let secret = read_secret("APPLICAMP_X", Some(&inline), Some(file.as_path()))
            .unwrap()
            .unwrap();
        assert_eq!(secret.expose_secret(), "from-file");
        assert!(read_secret("APPLICAMP_X", None, None).unwrap().is_none());
        std::fs::write(&file, " \n").unwrap();
        let err = read_secret("APPLICAMP_X", None, Some(file.as_path())).unwrap_err();
        assert!(err.to_string().contains("APPLICAMP_X"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
