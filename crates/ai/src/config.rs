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
//! - `APPLICAMP_AI__COMPLETE__INCLUDE_USAGE`: ask the provider for each
//!   call's cost (`"usage": {"include": true}`, OpenRouter); default `true`,
//!   `false` for a provider that refuses unknown fields.
//! - `APPLICAMP_AI__COMPLETE__PRICE_IN_USD_PER_MTOK` and `…_PRICE_OUT_…`: the
//!   model's price in US dollars per million tokens, when the built-in table
//!   ([`crate::price`]) does not know it.
//! - Spending limits in euros ([`BudgetConfig`]):
//!   `APPLICAMP_AI__BUDGET__ACCOUNT_DAILY_EUR` (default 0.30),
//!   `…__ACCOUNT_MONTHLY_EUR` (3), `…__INSTANCE_MONTHLY_EUR` (15), each `0`
//!   for no limit, and `…__USD_TO_EUR` (0.92), the fixed rate dollars are
//!   converted at.
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
/// What one account may spend per UTC day, in euros, by default.
pub const DEFAULT_ACCOUNT_DAILY_EUR: f64 = 0.30;
/// What one account may spend per calendar month (UTC), in euros, by default.
pub const DEFAULT_ACCOUNT_MONTHLY_EUR: f64 = 3.0;
/// What the whole instance may spend per calendar month (UTC), in euros, by
/// default: every account and every role together.
pub const DEFAULT_INSTANCE_MONTHLY_EUR: f64 = 15.0;
/// Euros per US dollar when nothing else is set.
pub const DEFAULT_USD_TO_EUR: f64 = 0.92;

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
    /// Ask for each call's cost in the answer (`"usage": {"include":
    /// true}`); `None` is yes.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_usage: Option<bool>,
    /// The input price in US dollars per million tokens, over the built-in
    /// table.
    #[serde(default, deserialize_with = "lenient_f64")]
    pub price_in_usd_per_mtok: Option<f64>,
    /// The output price in US dollars per million tokens, over the built-in
    /// table.
    #[serde(default, deserialize_with = "lenient_f64")]
    pub price_out_usd_per_mtok: Option<f64>,
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
            include_usage: None,
            price_in_usd_per_mtok: None,
            price_out_usd_per_mtok: None,
        }
    }

    /// The same provider at this price, in US dollars per million tokens.
    #[must_use]
    pub fn with_price(mut self, in_per_mtok: f64, out_per_mtok: f64) -> Self {
        self.price_in_usd_per_mtok = Some(in_per_mtok);
        self.price_out_usd_per_mtok = Some(out_per_mtok);
        self
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
    /// The spending limits (`APPLICAMP_AI__BUDGET__…`).
    #[serde(default)]
    pub budget: BudgetConfig,
}

/// The spending limits, in euros. Every model call is checked against them
/// before it is sent ([`crate::Gateway::complete`]). Read them through the
/// accessors, which apply the defaults; `0` is no limit.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct BudgetConfig {
    /// One account, per UTC day (`…__ACCOUNT_DAILY_EUR`).
    #[serde(default, deserialize_with = "lenient_f64")]
    pub account_daily_eur: Option<f64>,
    /// One account, per calendar month (`…__ACCOUNT_MONTHLY_EUR`).
    #[serde(default, deserialize_with = "lenient_f64")]
    pub account_monthly_eur: Option<f64>,
    /// The whole instance, per calendar month (`…__INSTANCE_MONTHLY_EUR`).
    #[serde(default, deserialize_with = "lenient_f64")]
    pub instance_monthly_eur: Option<f64>,
    /// Euros per US dollar (`…__USD_TO_EUR`).
    #[serde(default, deserialize_with = "lenient_f64")]
    pub usd_to_eur: Option<f64>,
}

/// `None` (unset) is `default`; zero or less is no limit.
fn limit(value: Option<f64>, default: f64) -> Option<f64> {
    let value = value.unwrap_or(default);
    (value > 0.0).then_some(value)
}

impl BudgetConfig {
    /// One account's daily limit in euros; `None` is no limit.
    pub fn account_daily_eur(&self) -> Option<f64> {
        limit(self.account_daily_eur, DEFAULT_ACCOUNT_DAILY_EUR)
    }

    /// One account's monthly limit in euros; `None` is no limit.
    pub fn account_monthly_eur(&self) -> Option<f64> {
        limit(self.account_monthly_eur, DEFAULT_ACCOUNT_MONTHLY_EUR)
    }

    /// The instance's monthly limit in euros; `None` is no limit.
    pub fn instance_monthly_eur(&self) -> Option<f64> {
        limit(self.instance_monthly_eur, DEFAULT_INSTANCE_MONTHLY_EUR)
    }

    /// Euros per US dollar.
    pub fn usd_to_eur(&self) -> f64 {
        self.usd_to_eur
            .filter(|rate| *rate > 0.0)
            .unwrap_or(DEFAULT_USD_TO_EUR)
    }
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

/// A number, as a number or as text; blank is unset.
fn lenient_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(f64),
        Text(String),
    }
    match Option::<Raw>::deserialize(d)? {
        None => Ok(None),
        Some(Raw::Number(n)) if n.is_finite() => Ok(Some(n)),
        Some(Raw::Number(n)) => Err(serde::de::Error::custom(format!(
            "{n} is not a usable number"
        ))),
        Some(Raw::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Raw::Text(text)) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom(format!("`{text}` is not a number"))),
    }
}

/// A yes or no, as a boolean or as text; blank is unset.
fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Bool(bool),
        Text(String),
    }
    match Option::<Raw>::deserialize(d)? {
        None => Ok(None),
        Some(Raw::Bool(b)) => Ok(Some(b)),
        Some(Raw::Text(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "" => Ok(None),
            "true" | "1" | "yes" => Ok(Some(true)),
            "false" | "0" | "no" => Ok(Some(false)),
            _ => Err(serde::de::Error::custom(format!(
                "`{text}` is not true or false"
            ))),
        },
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
    fn the_budget_has_defaults_and_zero_is_no_limit() {
        let config = load(serde_json::json!({})).unwrap();
        assert_eq!(
            config.budget.account_daily_eur(),
            Some(DEFAULT_ACCOUNT_DAILY_EUR)
        );
        assert_eq!(
            config.budget.account_monthly_eur(),
            Some(DEFAULT_ACCOUNT_MONTHLY_EUR)
        );
        assert_eq!(
            config.budget.instance_monthly_eur(),
            Some(DEFAULT_INSTANCE_MONTHLY_EUR)
        );
        assert!((config.budget.usd_to_eur() - DEFAULT_USD_TO_EUR).abs() < f64::EPSILON);

        let config = load(serde_json::json!({ "ai": {
            "budget": {
                "account_daily_eur": "0.10",
                "account_monthly_eur": 0,
                "instance_monthly_eur": "",
                "usd_to_eur": "0.9",
            },
            "complete": {
                "base_url": "https://x.example.com/v1",
                "model": "m",
                "include_usage": "false",
                "price_in_usd_per_mtok": "0.5",
                "price_out_usd_per_mtok": 2,
            },
        }}))
        .unwrap();
        assert_eq!(config.budget.account_daily_eur(), Some(0.10));
        assert_eq!(config.budget.account_monthly_eur(), None);
        assert_eq!(
            config.budget.instance_monthly_eur(),
            Some(DEFAULT_INSTANCE_MONTHLY_EUR)
        );
        assert!((config.budget.usd_to_eur() - 0.9).abs() < f64::EPSILON);
        let complete = config.provider(Role::Complete).unwrap();
        assert_eq!(complete.include_usage, Some(false));
        assert_eq!(complete.price_in_usd_per_mtok, Some(0.5));
        assert_eq!(complete.price_out_usd_per_mtok, Some(2.0));

        let err = load(serde_json::json!({ "ai": { "budget": { "account_daily_eur": "lots" } } }))
            .unwrap_err();
        assert!(err.to_string().contains("lots"), "{err}");
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
