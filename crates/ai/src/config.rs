//! Which provider and model serve each role, and the daily token cap.
//!
//! From the environment:
//!
//! - `APPLICAMP_AI__COMPLETE__BASE_URL`: the API base **including the version
//!   prefix**, as OpenAI SDKs expect (`https://openrouter.ai/api/v1`,
//!   `http://ollama:11434/v1`); the gateway appends `chat/completions`.
//! - `APPLICAMP_AI__COMPLETE__MODEL`: the model as the provider names it.
//! - `APPLICAMP_AI__COMPLETE__API_KEY_FILE` (preferred) or
//!   `APPLICAMP_AI__COMPLETE__API_KEY`: the bearer key, if the provider needs
//!   one.
//! - `APPLICAMP_AI__DAILY_TOKENS`: tokens (in + out) one account may use per
//!   UTC day; default [`DEFAULT_DAILY_TOKENS`], `0` for no cap.
//!
//! A role with neither a base URL nor a model is not configured, and the
//! features that need it say so ([`crate::Error::NotConfigured`]).

use std::{fmt, path::PathBuf};

use applicamp_kernel::config::{env_source, read_secret};
use figment::{
    Figment,
    providers::Serialized,
    value::{Dict, Value},
};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::Error;

/// Tokens one account may use per UTC day when nothing else is set.
pub const DEFAULT_DAILY_TOKENS: u64 = 200_000;

/// What a call is *for*. Code asks for a role, never for a vendor or model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Free-form generation and structuring (a CV into a profile).
    Complete,
}

impl Role {
    /// Lower-case name, as in config keys and the usage ledger.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An OpenAI-compatible endpoint and the model to use on it.
#[derive(Clone, Deserialize)]
pub struct ProviderConfig {
    /// API base URL including the version prefix (`…/v1`).
    pub base_url: Url,
    /// Model identifier as the provider names it.
    pub model: String,
    /// Bearer key, if the provider needs one. Never logged.
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// A file holding the key; wins over [`api_key`](Self::api_key). Read
    /// once, when the configuration loads.
    #[serde(default)]
    pub api_key_file: Option<PathBuf>,
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("base_url", &self.base_url.as_str())
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

impl ProviderConfig {
    /// `model` at `base_url`, with no key.
    pub fn new(base_url: Url, model: impl Into<String>) -> Self {
        Self {
            base_url,
            model: model.into(),
            api_key: None,
            api_key_file: None,
        }
    }

    /// The same provider with a bearer key.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(SecretString::from(key.into()));
        self
    }
}

/// The AI configuration: a provider per role (any may be missing) and the
/// daily cap.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AiConfig {
    /// The `complete` role (`APPLICAMP_AI__COMPLETE__…`).
    #[serde(default)]
    pub complete: Option<ProviderConfig>,
    /// Tokens per account per UTC day (`APPLICAMP_AI__DAILY_TOKENS`); read it
    /// through [`daily_token_cap`](Self::daily_token_cap), which applies the
    /// default.
    #[serde(default, deserialize_with = "daily_tokens")]
    pub daily_tokens: Option<u64>,
}

impl AiConfig {
    /// The configuration in the `APPLICAMP_AI__…` variables.
    pub fn from_env() -> Result<Self, Error> {
        Self::from_figment(&env_source())
    }

    /// The `ai` section of `figment`. A role with neither a base URL nor a
    /// model (both unset, or left empty in `.env`) is not configured; one
    /// with only one of them is an error naming the missing setting.
    pub fn from_figment(figment: &Figment) -> Result<Self, Error> {
        if !figment.contains("ai") {
            return Ok(Self::default());
        }
        let mut section: Dict = figment
            .extract_inner("ai")
            .map_err(|e| Error::Config(e.to_string()))?;
        if let Some(Value::Dict(_, role)) = section.get_mut(Role::Complete.as_str()) {
            role.retain(|_, value| !is_blank(value));
            if !role.contains_key("base_url") && !role.contains_key("model") {
                section.remove(Role::Complete.as_str());
            }
        }
        let mut config: Self = Figment::from(Serialized::defaults(section))
            .extract()
            .map_err(|e| Error::Config(format!("APPLICAMP_AI__…: {e}")))?;
        if let Some(provider) = config.complete.as_mut() {
            provider.api_key = read_secret(
                "APPLICAMP_AI__COMPLETE__API_KEY",
                provider.api_key.as_ref(),
                provider.api_key_file.as_deref(),
            )
            .map_err(|e| Error::Config(e.to_string()))?;
            provider.api_key_file = None;
        }
        Ok(config)
    }

    /// The provider for `role`.
    pub fn provider(&self, role: Role) -> Result<&ProviderConfig, Error> {
        match role {
            Role::Complete => self.complete.as_ref(),
        }
        .ok_or(Error::NotConfigured(role))
    }

    /// Whether `role` has a provider.
    pub fn is_configured(&self, role: Role) -> bool {
        self.provider(role).is_ok()
    }

    /// Tokens one account may use per UTC day; `None` is no cap.
    pub fn daily_token_cap(&self) -> Option<u64> {
        match self.daily_tokens {
            None => Some(DEFAULT_DAILY_TOKENS),
            Some(0) => None,
            Some(cap) => Some(cap),
        }
    }
}

/// A value left empty in `.env` (`APPLICAMP_AI__COMPLETE__MODEL=`).
fn is_blank(value: &Value) -> bool {
    matches!(value, Value::String(_, text) if text.trim().is_empty())
}

/// A whole number, as a number or as text; blank is unset.
fn daily_tokens<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Text(String),
    }
    match Option::<Raw>::deserialize(d)? {
        None => Ok(None),
        Some(Raw::Number(n)) => Ok(Some(n)),
        Some(Raw::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Raw::Text(text)) => text.trim().parse().map(Some).map_err(|_| {
            serde::de::Error::custom(format!(
                "DAILY_TOKENS must be a whole number (0 for no cap), not `{text}`"
            ))
        }),
    }
}

/// The key, for the `Authorization` header.
pub(crate) fn bearer(provider: &ProviderConfig) -> Option<&str> {
    provider.api_key.as_ref().map(|key| key.expose_secret())
}

#[cfg(test)]
mod tests {
    use figment::providers::Serialized;

    use super::*;

    fn load(value: serde_json::Value) -> Result<AiConfig, Error> {
        AiConfig::from_figment(&Figment::from(Serialized::defaults(value)))
    }

    #[test]
    fn nothing_set_is_nothing_configured() {
        let config = load(serde_json::json!({})).unwrap();
        assert!(!config.is_configured(Role::Complete));
        assert!(matches!(
            config.provider(Role::Complete),
            Err(Error::NotConfigured(Role::Complete))
        ));
        assert_eq!(config.daily_token_cap(), Some(DEFAULT_DAILY_TOKENS));
    }

    #[test]
    fn the_complete_role_is_read() {
        let config = load(serde_json::json!({ "ai": {
            "complete": {
                "base_url": "https://openrouter.example.com/api/v1",
                "model": "some/model",
                "api_key": "sk-test",
            },
            "daily_tokens": "50000",
        }}))
        .unwrap();
        let complete = config.provider(Role::Complete).unwrap();
        assert_eq!(complete.model, "some/model");
        assert_eq!(bearer(complete), Some("sk-test"));
        assert_eq!(config.daily_token_cap(), Some(50_000));
    }

    #[test]
    fn blank_settings_configure_nothing() {
        let config = load(serde_json::json!({ "ai": {
            "complete": { "base_url": "", "model": " ", "api_key": "" },
            "daily_tokens": "",
        }}))
        .unwrap();
        assert!(!config.is_configured(Role::Complete));
        assert_eq!(config.daily_token_cap(), Some(DEFAULT_DAILY_TOKENS));
    }

    #[test]
    fn a_half_configured_role_is_an_error() {
        let err = load(serde_json::json!({ "ai": {
            "complete": { "base_url": "https://x.example.com/v1" },
        }}))
        .unwrap_err();
        assert!(err.to_string().contains("model"), "{err}");
    }

    #[test]
    fn zero_is_no_cap_and_nonsense_is_refused() {
        let config = load(serde_json::json!({ "ai": { "daily_tokens": 0 } })).unwrap();
        assert_eq!(config.daily_token_cap(), None);
        let err = load(serde_json::json!({ "ai": { "daily_tokens": "lots" } })).unwrap_err();
        assert!(err.to_string().contains("DAILY_TOKENS"), "{err}");
    }

    #[test]
    fn the_key_file_wins_and_is_never_printed() {
        let dir = std::env::temp_dir().join(format!("applicamp-ai-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("key");
        std::fs::write(&file, "sk-from-file\n").unwrap();
        let config = load(serde_json::json!({ "ai": { "complete": {
            "base_url": "https://x.example.com/v1",
            "model": "m",
            "api_key": "sk-inline",
            "api_key_file": file,
        }}}))
        .unwrap();
        let complete = config.provider(Role::Complete).unwrap();
        assert_eq!(bearer(complete), Some("sk-from-file"));
        assert!(!format!("{config:?}").contains("sk-from-file"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
