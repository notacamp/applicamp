//! `applicamp-runs`: every agent runs in its own sandbox, and you can watch
//! it (ADR 0002, 0003, 0009).
//!
//! - **Runs** ([`features::runs`]): the queue (`runs.runs`, claimed by the
//!   [`orchestrator`] loop the server starts), the lifecycle (queued →
//!   starting → running → finished with one [`Outcome`]), one run at a time
//!   per account and three on the instance, a deadline per kind, Stop, and
//!   a sweep of left-over sandboxes at startup. What a run did is its
//!   events: the agent's own and the sandbox's allow and deny lines, live
//!   over SSE, kept 30 days ([`retention`]).
//! - **Kinds** are declared by the module that owns their subject, with the
//!   kernel's types ([`applicamp_kernel::agent`]); `runs` only executes
//!   them. Its own is [`probe`], "Run a check". A module starts a run
//!   through the server's [`applicamp_kernel::agent::RunStarter`] and hears
//!   the end by consuming `runs.finished`.
//! - **Sandboxes** come from a [`runtime::SandboxRuntime`]: OpenShell, or a
//!   fake for tests and local development. A sandbox holds only its run id,
//!   a single-run token and the agent API's address.
//! - **The agent API** ([`features::agent_api`], `/api/agent/v1/…`) is the
//!   only way a sandbox reaches the server: bearer run tokens naming the
//!   kind's tools with their caps, never a cookie, outside the CSRF check.
//! - **The gateway's issuer** ([`issuer`], [`features::oidc`]): OpenShell
//!   trusts this instance's own five-minute tokens.
//!
//! Logs carry run ids, kinds, statuses, outcomes, durations and counts;
//! never event messages, tool arguments or results. Publishes
//! `runs.finished` (ids and the outcome).

#![forbid(unsafe_code)]

pub mod config;
pub mod features;
pub mod issuer;
pub mod kinds;
pub mod orchestrator;
pub mod probe;
pub mod retention;
pub mod runtime;
pub mod schema;
pub mod store;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use applicamp_ai::Gateway;
use applicamp_kernel::{
    EventBus, Module,
    agent::{Outcome, RunFinished, RunKind},
    crypto::SecretKey,
    jobs::Background,
};
use axum::Extension;
use sqlx::PgPool;
use tokio::{sync::watch, task::JoinHandle};
use url::Url;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

pub use config::{RunsConfig, RuntimeKind};
pub use kinds::Kinds;
pub use runtime::SandboxRuntime;

/// The module's name, schema and operation prefix.
pub const NAME: &str = "runs";
/// The OpenAPI security scheme of the agent API: `security(("run_token" = []))`.
pub const RUN_TOKEN_SCHEME: &str = "run_token";
/// Where the agent API lives; no other path takes a run token.
pub const AGENT_API_PREFIX: &str = "/api/agent/";
/// Requests a second one run may make to the agent API.
pub const AGENT_REQUESTS_PER_SECOND: u32 = 10;

/// The kinds `runs` owns itself.
pub fn own_kinds() -> Vec<RunKind> {
    vec![probe::kind()]
}

/// Counts each run's agent API requests per second.
#[derive(Debug, Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<Uuid, (Instant, u32)>>,
}

impl RateLimiter {
    /// Whether run `run` may make one more request now.
    pub fn allow(&self, run: Uuid) -> bool {
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if windows.len() > 1_000 {
            windows.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(10));
        }
        let window = windows.entry(run).or_insert((now, 0));
        if now.duration_since(window.0) >= Duration::from_secs(1) {
            *window = (now, 0);
        }
        window.1 += 1;
        window.1 <= AGENT_REQUESTS_PER_SECOND
    }
}

/// What the module is composed with.
#[derive(Clone)]
pub struct RunsDeps {
    /// The sandbox settings.
    pub config: Arc<RunsConfig>,
    /// Every kind this instance executes.
    pub kinds: Arc<Kinds>,
    /// Where sandboxes come from; none means runs cannot start.
    pub runtime: Option<Arc<dyn SandboxRuntime>>,
    /// Where agents reach the agent API.
    pub api: Option<Url>,
    /// The OpenShell gateway's issuer (with the OpenShell runtime only).
    pub issuer: Option<Arc<issuer::Issuer>>,
    /// The AI gateway behind the agent API's `complete`.
    pub ai: Option<Gateway>,
    /// The agent API's rate limit.
    pub limiter: Arc<RateLimiter>,
    /// This server, as it claims runs.
    pub instance: Arc<str>,
}

impl Default for RunsDeps {
    fn default() -> Self {
        Self::new(RunsConfig::default(), None)
    }
}

impl std::fmt::Debug for RunsDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunsDeps")
            .field("runtime", &self.runtime.as_ref().map(|r| r.name()))
            .field(
                "kinds",
                &self.kinds.iter().map(|k| k.name).collect::<Vec<_>>(),
            )
            .field("ai", &self.ai.is_some())
            .finish_non_exhaustive()
    }
}

impl RunsDeps {
    /// Composed over `config` and `ai`, with no runtime yet (see
    /// [`with_secret`](Self::with_secret)) and `runs`' own kinds.
    pub fn new(config: RunsConfig, ai: Option<Gateway>) -> Self {
        Self {
            config: Arc::new(config),
            kinds: Arc::new(Kinds::new([])),
            runtime: None,
            api: None,
            issuer: None,
            ai,
            limiter: Arc::new(RateLimiter::default()),
            instance: format!("server-{}", Uuid::now_v7().simple()).into(),
        }
    }

    /// The same, with the configured runtime: checked against the public
    /// URL `public_url` (no fake in production; OpenShell needs a gateway
    /// and https), the gateway's issuer keyed from the instance `secret`.
    pub fn with_secret(mut self, secret: &SecretKey, public_url: &str) -> Result<Self, String> {
        let public = Url::parse(public_url)
            .map_err(|e| format!("APPLICAMP_PUBLIC_URL={public_url}: {e}"))?;
        self.config.check(&public)?;
        let api = self.config.agent_api(&public);
        let runtime: Option<Arc<dyn SandboxRuntime>> = match self.config.runtime {
            None => None,
            Some(RuntimeKind::Fake) => Some(Arc::new(runtime::fake::FakeRuntime::new(
                self.config.fake_agent(),
            ))),
            Some(RuntimeKind::Openshell) => {
                let issuer = Arc::new(issuer::Issuer::new(
                    secret,
                    &public.origin().ascii_serialization(),
                )?);
                let ca = match &self.config.gateway_ca_file {
                    Some(path) => Some(std::fs::read(path).map_err(|e| {
                        format!("APPLICAMP_SANDBOX__GATEWAY_CA_FILE={}: {e}", path.display())
                    })?),
                    None => None,
                };
                self.issuer = Some(Arc::clone(&issuer));
                Some(Arc::new(runtime::openshell::OpenShellRuntime::new(
                    self.config.gateway_url.clone().unwrap_or_default(),
                    ca,
                    issuer,
                    self.config.agent_image().to_owned(),
                )))
            }
        };
        if runtime.is_some() {
            self.api = Some(api);
        }
        self.runtime = runtime;
        Ok(self)
    }

    /// The same, running sandboxes on `runtime` with the agent API at `api`
    /// (tests).
    #[must_use]
    pub fn with_runtime(mut self, runtime: Arc<dyn SandboxRuntime>, api: Url) -> Self {
        self.runtime = Some(runtime);
        self.api = Some(api);
        self
    }

    /// The same, executing `kinds` (every module's) besides `runs`' own.
    #[must_use]
    pub fn with_kinds(mut self, kinds: impl IntoIterator<Item = RunKind>) -> Self {
        self.kinds = Arc::new(Kinds::new(kinds));
        self
    }

    /// The runtime's name, if one is configured (for the startup log).
    pub fn runtime_name(&self) -> Option<&'static str> {
        self.runtime.as_ref().map(|runtime| runtime.name())
    }
}

/// The runs module, composed with `deps`.
pub fn module(deps: RunsDeps) -> Module {
    use features::{agent_api, oidc, runs};
    let router = OpenApiRouter::new()
        .routes(routes!(runs::list, runs::start))
        .routes(routes!(runs::get))
        .routes(routes!(runs::cancel))
        .routes(routes!(runs::events))
        .routes(routes!(runs::stream))
        .routes(routes!(agent_api::context))
        .routes(routes!(agent_api::events))
        .routes(routes!(agent_api::tool))
        .routes(routes!(agent_api::complete))
        .routes(routes!(agent_api::finish))
        .routes(routes!(oidc::discovery))
        .routes(routes!(oidc::jwks))
        .layer(Extension(deps));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job::<retention::PruneEvents>()
            .publishes::<RunFinished>(),
        kinds: own_kinds(),
    }
}

/// On server start: make what every kind needs at the runtime (OpenShell's
/// templates), end the runs a dead server left going, remove left-over
/// sandboxes, schedule the event retention, and start the orchestrator
/// loop until `shutdown`. Without a runtime, only the retention.
pub async fn start(
    db: &PgPool,
    events: &EventBus,
    deps: &RunsDeps,
    shutdown: watch::Receiver<bool>,
) -> Result<Option<JoinHandle<()>>, String> {
    retention::schedule(db).await.map_err(|e| e.to_string())?;
    let Some(orchestrator) =
        orchestrator::Orchestrator::new(db.clone(), events.clone(), deps.clone())
    else {
        return Ok(None);
    };
    if let Err(e) = orchestrator.runtime().prepare(&deps.kinds).await {
        // The gateway may come up later; runs fail until it does.
        tracing::error!(error = %e, "the sandbox runtime could not be prepared");
    }
    orchestrator.startup().await;
    Ok(Some(tokio::spawn(orchestrator.run(shutdown))))
}

/// The outcome an agent may finish with itself.
pub(crate) fn agent_outcomes() -> [Outcome; 4] {
    [
        Outcome::Succeeded,
        Outcome::NothingNew,
        Outcome::NeedsYou,
        Outcome::Failed,
    ]
}

#[cfg(test)]
pub(crate) mod testkit;

#[cfg(test)]
mod lifecycle;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_is_named_after_its_schema_and_owns_the_probe() {
        let module = module(RunsDeps::default());
        assert_eq!(module.name, "runs");
        assert_eq!(module.kinds.len(), 1);
        assert_eq!(module.kinds[0].name, probe::KIND);
        let registry =
            applicamp_kernel::jobs::Registry::build(vec![(NAME, module.background)]).unwrap();
        assert_eq!(registry.kinds(), ["runs.prune_events"]);
    }

    #[test]
    fn a_run_may_make_ten_requests_a_second() {
        let limiter = RateLimiter::default();
        let run = Uuid::now_v7();
        for _ in 0..AGENT_REQUESTS_PER_SECOND {
            assert!(limiter.allow(run));
        }
        assert!(!limiter.allow(run));
        assert!(limiter.allow(Uuid::now_v7()), "per run");
    }

    #[test]
    fn no_runtime_unless_configured() {
        let secret = SecretKey::from_bytes([3; 32]);
        let deps = RunsDeps::default()
            .with_secret(&secret, "https://applicamp.example.com")
            .unwrap();
        assert!(deps.runtime.is_none());
        assert!(deps.issuer.is_none());
        let fake = RunsDeps::new(RunsConfig::fake("applicamp-agent".into()), None)
            .with_secret(&secret, "http://localhost:8080")
            .unwrap();
        assert_eq!(fake.runtime_name(), Some("fake"));
        assert_eq!(
            fake.api.as_ref().map(Url::as_str),
            Some("http://localhost:8080/")
        );
        assert!(
            RunsDeps::new(RunsConfig::fake("applicamp-agent".into()), None)
                .with_secret(&secret, "https://applicamp.example.com")
                .is_err()
        );
    }
}
