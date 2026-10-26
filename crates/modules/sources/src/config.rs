//! Where the job boards are and how searches run, from the environment
//! (`APPLICAMP_SOURCES__…`). Every setting has a default; the API addresses
//! are the boards' own, changed only to point at a fake in tests.
//!
//! | Variable | Default |
//! |---|---|
//! | `APPLICAMP_SOURCES__ENABLED` | `greenhouse,lever,ashby,arbeitnow,hn` |
//! | `APPLICAMP_SOURCES__GREENHOUSE_URL` | `https://boards-api.greenhouse.io` |
//! | `APPLICAMP_SOURCES__LEVER_URL` | `https://api.lever.co` |
//! | `APPLICAMP_SOURCES__LEVER_EU_URL` | `https://api.eu.lever.co` |
//! | `APPLICAMP_SOURCES__ASHBY_URL` | `https://api.ashbyhq.com` |
//! | `APPLICAMP_SOURCES__ARBEITNOW_URL` | `https://www.arbeitnow.com` |
//! | `APPLICAMP_SOURCES__HN_URL` | `https://hn.algolia.com` |
//! | `APPLICAMP_SOURCES__BOARDS_FILE` | the seed list built in (`boards.toml`) |
//! | `APPLICAMP_SOURCES__CACHE_SECS` | `21600` (6 hours; `0` never caches) |
//! | `APPLICAMP_SOURCES__REQUESTS_PER_SECOND` | `2` (per host) |
//! | `APPLICAMP_SOURCES__MANUAL_PER_DAY` | `3` (`0` for no limit) |
//! | `APPLICAMP_SOURCES__MANUAL_EVERY_SECS` | `900` (`0` for no spacing) |
//! | `APPLICAMP_SOURCES__NIGHTLY` | `true` (21:00 Europe/Lisbon) |

use std::path::PathBuf;

use applicamp_kernel::config::env_source;
use figment::Figment;
use serde::{Deserialize, Deserializer};
use url::Url;

use crate::job::Source;

/// The sources settings.
#[derive(Debug, Clone, Deserialize)]
pub struct SourcesConfig {
    /// The sources read, comma-separated; unset is all of them.
    #[serde(default, deserialize_with = "list")]
    pub enabled: Option<Vec<String>>,
    /// Greenhouse's job board API.
    #[serde(default = "greenhouse_url")]
    pub greenhouse_url: Url,
    /// Lever's postings API.
    #[serde(default = "lever_url")]
    pub lever_url: Url,
    /// Lever's postings API for boards on its EU instance.
    #[serde(default = "lever_eu_url")]
    pub lever_eu_url: Url,
    /// Ashby's posting API.
    #[serde(default = "ashby_url")]
    pub ashby_url: Url,
    /// Arbeitnow's job board API.
    #[serde(default = "arbeitnow_url")]
    pub arbeitnow_url: Url,
    /// HN's search API (Algolia).
    #[serde(default = "hn_url")]
    pub hn_url: Url,
    /// A boards list to use instead of the built-in seed list.
    #[serde(default)]
    pub boards_file: Option<PathBuf>,
    /// How long a board's answer is reused, in seconds.
    #[serde(default, deserialize_with = "number")]
    pub cache_secs: Option<u64>,
    /// Requests per second to one host.
    #[serde(default, deserialize_with = "number")]
    pub requests_per_second: Option<u64>,
    /// Manual searches per account per UTC day.
    #[serde(default, deserialize_with = "number")]
    pub manual_per_day: Option<u64>,
    /// Seconds between two manual searches of one account.
    #[serde(default, deserialize_with = "number")]
    pub manual_every_secs: Option<u64>,
    /// Search every night for accounts with goals.
    #[serde(default, deserialize_with = "flag")]
    pub nightly: Option<bool>,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            greenhouse_url: greenhouse_url(),
            lever_url: lever_url(),
            lever_eu_url: lever_eu_url(),
            ashby_url: ashby_url(),
            arbeitnow_url: arbeitnow_url(),
            hn_url: hn_url(),
            boards_file: None,
            cache_secs: None,
            requests_per_second: None,
            manual_per_day: None,
            manual_every_secs: None,
            nightly: None,
        }
    }
}

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap_or_else(|e| unreachable!("a built-in URL parses: {e}"))
}

fn greenhouse_url() -> Url {
    url("https://boards-api.greenhouse.io")
}
fn lever_url() -> Url {
    url("https://api.lever.co")
}
fn lever_eu_url() -> Url {
    url("https://api.eu.lever.co")
}
fn ashby_url() -> Url {
    url("https://api.ashbyhq.com")
}
fn arbeitnow_url() -> Url {
    url("https://www.arbeitnow.com")
}
fn hn_url() -> Url {
    url("https://hn.algolia.com")
}

impl SourcesConfig {
    /// The settings in the `APPLICAMP_SOURCES__…` variables.
    pub fn from_env() -> Result<Self, String> {
        Self::from_figment(&env_source())
    }

    /// The `sources` section of `figment`.
    pub fn from_figment(figment: &Figment) -> Result<Self, String> {
        if !figment.contains("sources") {
            return Ok(Self::default());
        }
        let config: Self = figment
            .extract_inner("sources")
            .map_err(|e| format!("APPLICAMP_SOURCES__…: {e}"))?;
        if let Some(unknown) = config
            .enabled
            .iter()
            .flatten()
            .find(|name| Source::parse(name).is_none())
        {
            return Err(format!(
                "APPLICAMP_SOURCES__ENABLED: `{unknown}` is not one of greenhouse, lever, \
                 ashby, arbeitnow, hn"
            ));
        }
        Ok(config)
    }

    /// Whether `source` is read.
    pub fn is_enabled(&self, source: Source) -> bool {
        self.enabled
            .as_ref()
            .is_none_or(|names| names.iter().any(|name| name == source.as_str()))
    }

    /// How long a board's answer is reused.
    pub fn cache_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.cache_secs.unwrap_or(6 * 3600))
    }

    /// The wait between two requests to one host.
    pub fn request_spacing(&self) -> std::time::Duration {
        let per_second = self.requests_per_second.unwrap_or(2).max(1);
        std::time::Duration::from_millis(1_000 / per_second)
    }

    /// Manual searches per account per day; `None` is no limit.
    pub fn manual_per_day(&self) -> Option<u64> {
        Some(self.manual_per_day.unwrap_or(3)).filter(|n| *n > 0)
    }

    /// The least time between two manual searches; `None` is none.
    pub fn manual_every(&self) -> Option<chrono::TimeDelta> {
        let secs = self.manual_every_secs.unwrap_or(15 * 60);
        (secs > 0).then(|| chrono::TimeDelta::seconds(i64::try_from(secs).unwrap_or(i64::MAX)))
    }

    /// Whether the nightly search runs.
    pub fn nightly(&self) -> bool {
        self.nightly.unwrap_or(true)
    }
}

/// Comma-separated names; blank is unset.
fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        List(Vec<String>),
        Text(String),
    }
    Ok(match Option::<Raw>::deserialize(d)? {
        None => None,
        Some(Raw::List(items)) => Some(items),
        Some(Raw::Text(text)) if text.trim().is_empty() => None,
        Some(Raw::Text(text)) => Some(
            text.split(',')
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect(),
        ),
    })
}

/// A whole number, as a number or as text; blank is unset.
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
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
        Some(Raw::Text(text)) => text
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| serde::de::Error::custom(format!("`{text}` is not a whole number"))),
    }
}

/// A yes or no, as a boolean or as text; blank is unset.
fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
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

#[cfg(test)]
mod tests {
    use figment::providers::Serialized;

    use super::*;

    fn load(value: serde_json::Value) -> Result<SourcesConfig, String> {
        SourcesConfig::from_figment(&Figment::from(Serialized::defaults(value)))
    }

    #[test]
    fn defaults_are_the_boards_own_apis_and_the_limits() {
        let config = load(serde_json::json!({})).unwrap();
        assert_eq!(
            config.greenhouse_url.as_str(),
            "https://boards-api.greenhouse.io/"
        );
        assert!(config.is_enabled(Source::Hn));
        assert_eq!(config.manual_per_day(), Some(3));
        assert_eq!(config.manual_every(), Some(chrono::TimeDelta::minutes(15)));
        assert_eq!(
            config.request_spacing(),
            std::time::Duration::from_millis(500)
        );
        assert!(config.nightly());
    }

    #[test]
    fn settings_are_read_and_zero_lifts_a_limit() {
        let config = load(serde_json::json!({ "sources": {
            "enabled": "greenhouse, Lever",
            "greenhouse_url": "http://fake-jobs:9020",
            "cache_secs": "0",
            "manual_per_day": 0,
            "manual_every_secs": "0",
            "nightly": "false",
        }}))
        .unwrap();
        assert!(config.is_enabled(Source::Greenhouse));
        assert!(config.is_enabled(Source::Lever));
        assert!(!config.is_enabled(Source::Hn));
        assert_eq!(config.greenhouse_url.as_str(), "http://fake-jobs:9020/");
        assert_eq!(config.cache_ttl(), std::time::Duration::ZERO);
        assert_eq!(config.manual_per_day(), None);
        assert_eq!(config.manual_every(), None);
        assert!(!config.nightly());
        let err = load(serde_json::json!({ "sources": { "enabled": "linkedin" } })).unwrap_err();
        assert!(err.contains("linkedin"), "{err}");
    }
}
