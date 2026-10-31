//! Where sandboxes run, from the environment (`APPLICAMP_SANDBOX__…`).
//!
//! | Variable | Default |
//! |---|---|
//! | `APPLICAMP_SANDBOX__RUNTIME` | unset: no runtime, runs cannot start (`openshell` or `fake`) |
//! | `APPLICAMP_SANDBOX__GATEWAY_URL` | none: the OpenShell gateway (`https://…`), required with `openshell` |
//! | `APPLICAMP_SANDBOX__GATEWAY_CA_FILE` | the system roots: a PEM file pinning the gateway's private CA |
//! | `APPLICAMP_SANDBOX__AGENT_IMAGE` | `ghcr.io/notacamp/applicamp-agent:main` |
//! | `APPLICAMP_SANDBOX__AGENT_API_URL` | `APPLICAMP_PUBLIC_URL`: where agents reach the agent API |
//! | `APPLICAMP_SANDBOX__FAKE_AGENT` | `target/debug/applicamp-agent`: the binary the fake runtime starts |
//! | `APPLICAMP_SANDBOX__ALLOW_FAKE` | `false`: the fake runtime refuses an https public URL (production) |
//! | `APPLICAMP_SANDBOX__MAX_RUNS` | `3`: runs going at once on the instance |

use std::path::PathBuf;

use applicamp_kernel::config::env_source;
use figment::Figment;
use serde::{Deserialize, Deserializer};
use url::Url;

/// The image the runner runs in, unless configured.
pub const DEFAULT_AGENT_IMAGE: &str = "ghcr.io/notacamp/applicamp-agent:main";
/// The runner the fake runtime starts, unless configured (a debug build).
pub const DEFAULT_FAKE_AGENT: &str = "target/debug/applicamp-agent";
/// Runs going at once on the instance, unless configured.
pub const DEFAULT_MAX_RUNS: u32 = 3;
/// Runs going at once for one account.
pub const PER_ACCOUNT: i64 = 1;

/// Which sandbox runtime runs agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    /// NVIDIA OpenShell, through its gateway (ADR 0002).
    Openshell,
    /// The runner as a local child process behind a policy proxy: tests and
    /// local development only.
    Fake,
}

impl RuntimeKind {
    /// As configured and shown.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Openshell => "openshell",
            Self::Fake => "fake",
        }
    }
}

/// The sandbox settings.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunsConfig {
    /// The runtime; none means runs cannot start.
    #[serde(default, deserialize_with = "runtime")]
    pub runtime: Option<RuntimeKind>,
    /// The OpenShell gateway's address.
    #[serde(default)]
    pub gateway_url: Option<String>,
    /// A PEM file with the gateway's private CA.
    #[serde(default)]
    pub gateway_ca_file: Option<PathBuf>,
    /// The runner's image.
    #[serde(default)]
    pub agent_image: Option<String>,
    /// Where agents reach the agent API, when not at the public URL.
    #[serde(default)]
    pub agent_api_url: Option<Url>,
    /// The runner the fake runtime starts.
    #[serde(default)]
    pub fake_agent: Option<PathBuf>,
    /// Let the fake runtime run although the public URL is https.
    #[serde(default, deserialize_with = "flag")]
    pub allow_fake: Option<bool>,
    /// Runs going at once on the instance.
    #[serde(default, deserialize_with = "number")]
    pub max_runs: Option<u32>,
}

impl RunsConfig {
    /// The settings in the `APPLICAMP_SANDBOX__…` variables.
    pub fn from_env() -> Result<Self, String> {
        Self::from_figment(&env_source())
    }

    /// The `sandbox` section of `figment`.
    pub fn from_figment(figment: &Figment) -> Result<Self, String> {
        if !figment.contains("sandbox") {
            return Ok(Self::default());
        }
        figment
            .extract_inner("sandbox")
            .map_err(|e| format!("APPLICAMP_SANDBOX__…: {e}"))
    }

    /// A configuration running agents with the fake runtime from `agent`
    /// (tests).
    pub fn fake(agent: PathBuf) -> Self {
        Self {
            runtime: Some(RuntimeKind::Fake),
            fake_agent: Some(agent),
            ..Self::default()
        }
    }

    /// The runner's image.
    pub fn agent_image(&self) -> &str {
        self.agent_image.as_deref().unwrap_or(DEFAULT_AGENT_IMAGE)
    }

    /// The runner the fake runtime starts.
    pub fn fake_agent(&self) -> PathBuf {
        self.fake_agent
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_FAKE_AGENT))
    }

    /// Runs going at once on the instance (at least one).
    pub fn max_runs(&self) -> i64 {
        i64::from(self.max_runs.unwrap_or(DEFAULT_MAX_RUNS).max(1))
    }

    /// Where agents reach the agent API: the configured address, else
    /// `public_url`.
    pub fn agent_api(&self, public_url: &Url) -> Url {
        self.agent_api_url
            .clone()
            .unwrap_or_else(|| public_url.clone())
    }

    /// Whether these settings may run with the public URL `public_url`: the
    /// fake runtime never in production (an https public URL) unless
    /// allowed, OpenShell only with a gateway and an https public URL (the
    /// gateway fetches this instance's keys from it).
    pub fn check(&self, public_url: &Url) -> Result<(), String> {
        let https = public_url.scheme() == "https";
        match self.runtime {
            None => Ok(()),
            Some(RuntimeKind::Fake) if https && !self.allow_fake.unwrap_or(false) => Err(
                "APPLICAMP_SANDBOX__RUNTIME=fake runs agents without a sandbox and is refused \
                 with an https APPLICAMP_PUBLIC_URL (set APPLICAMP_SANDBOX__ALLOW_FAKE=true to \
                 run it anyway)"
                    .to_owned(),
            ),
            Some(RuntimeKind::Fake) => Ok(()),
            Some(RuntimeKind::Openshell) => {
                let gateway = self.gateway_url.as_deref().unwrap_or_default().trim();
                if gateway.is_empty() {
                    return Err("APPLICAMP_SANDBOX__GATEWAY_URL is required with \
                         APPLICAMP_SANDBOX__RUNTIME=openshell"
                        .to_owned());
                }
                if Url::parse(gateway).is_err() {
                    return Err(format!(
                        "APPLICAMP_SANDBOX__GATEWAY_URL={gateway} is not a URL"
                    ));
                }
                if !https {
                    return Err(
                        "APPLICAMP_SANDBOX__RUNTIME=openshell needs an https APPLICAMP_PUBLIC_URL: \
                         the gateway trusts this instance as its OIDC issuer there"
                            .to_owned(),
                    );
                }
                Ok(())
            }
        }
    }
}

/// A runtime name; blank is unset.
fn runtime<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RuntimeKind>, D::Error> {
    match Option::<String>::deserialize(d)? {
        None => Ok(None),
        Some(text) => match text.trim().to_ascii_lowercase().as_str() {
            "" | "none" => Ok(None),
            "openshell" => Ok(Some(RuntimeKind::Openshell)),
            "fake" => Ok(Some(RuntimeKind::Fake)),
            other => Err(serde::de::Error::custom(format!(
                "`{other}` is not a runtime (openshell or fake)"
            ))),
        },
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
            "true" | "1" | "yes" | "on" => Ok(Some(true)),
            "false" | "0" | "no" | "off" => Ok(Some(false)),
            other => Err(serde::de::Error::custom(format!(
                "`{other}` is not a yes or no"
            ))),
        },
    }
}

/// A whole number, as a number or as text; blank is unset.
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u32),
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

#[cfg(test)]
mod tests {
    use figment::providers::Serialized;

    use super::*;

    fn config(json: serde_json::Value) -> Result<RunsConfig, String> {
        RunsConfig::from_figment(&Figment::from(Serialized::defaults(json)))
    }

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn unset_is_no_runtime() {
        let config = config(serde_json::json!({})).unwrap();
        assert_eq!(config.runtime, None);
        assert_eq!(config.agent_image(), DEFAULT_AGENT_IMAGE);
        assert_eq!(config.max_runs(), 3);
        assert!(config.check(&url("https://applicamp.example.com")).is_ok());
    }

    #[test]
    fn the_fake_is_refused_in_production_unless_allowed() {
        let fake = config(serde_json::json!({ "sandbox": { "runtime": "fake" } })).unwrap();
        assert_eq!(fake.runtime, Some(RuntimeKind::Fake));
        assert!(fake.check(&url("http://localhost:8080")).is_ok());
        let err = fake
            .check(&url("https://applicamp.example.com"))
            .unwrap_err();
        assert!(err.contains("ALLOW_FAKE"), "{err}");
        let allowed = config(serde_json::json!({
            "sandbox": { "runtime": "fake", "allow_fake": "true" }
        }))
        .unwrap();
        assert!(allowed.check(&url("https://applicamp.example.com")).is_ok());
    }

    #[test]
    fn openshell_needs_a_gateway_and_https() {
        let none = config(serde_json::json!({ "sandbox": { "runtime": "openshell" } })).unwrap();
        assert!(
            none.check(&url("https://applicamp.example.com"))
                .unwrap_err()
                .contains("GATEWAY_URL")
        );
        let gateway = config(serde_json::json!({ "sandbox": {
            "runtime": "openshell", "gateway_url": "https://gateway.example.com:8443",
        }}))
        .unwrap();
        assert!(gateway.check(&url("https://applicamp.example.com")).is_ok());
        assert!(gateway.check(&url("http://localhost:8080")).is_err());
        assert!(config(serde_json::json!({ "sandbox": { "runtime": "docker" } })).is_err());
    }

    #[test]
    fn the_agent_api_is_the_public_url_unless_set() {
        let public = url("https://applicamp.example.com");
        assert_eq!(RunsConfig::default().agent_api(&public), public);
        let set = config(serde_json::json!({ "sandbox": {
            "agent_api_url": "https://agents.example.com",
        }}))
        .unwrap();
        assert_eq!(
            set.agent_api(&public).as_str(),
            "https://agents.example.com/"
        );
    }
}
