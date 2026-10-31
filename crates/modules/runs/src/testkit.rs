//! The module over a real Postgres (`#[sqlx::test]`): its routes signed in
//! as an invented account, the agent API as a sandbox calls it, and the
//! orchestrator with the fake runtime.

use std::{path::PathBuf, sync::Arc, time::Duration};

use applicamp_kernel::{
    AppState, Config,
    agent::{Image, Limits, RunKind},
    auth::Principal,
    crypto::{self, SecretKey},
    events::EventBus,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
};
use chrono::Utc;
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use url::Url;
use uuid::Uuid;

use crate::{
    RunsConfig, RunsDeps, module,
    orchestrator::Orchestrator,
    probe,
    runtime::{SandboxRuntime, fake::FakeRuntime},
    store,
};

/// The agent runner the CI builds before the tests
/// (`cargo build -p applicamp-agent`).
pub(crate) fn agent_binary() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target"),
        PathBuf::from,
    );
    let binary = target.join("debug/applicamp-agent");
    assert!(
        binary.is_file(),
        "{} is missing: build it first (cargo build -p applicamp-agent)",
        binary.display()
    );
    binary
}

/// A fake runtime whose children run `script` with `sh`.
pub(crate) fn scripted(script: &str) -> Arc<FakeRuntime> {
    Arc::new(FakeRuntime::with_command(
        PathBuf::from("/bin/sh"),
        vec!["-c".to_owned(), script.to_owned(), "sh".to_owned()],
    ))
}

/// A kind for tests: `runs.<name>`, with `wall_clock` and `max_tokens`,
/// the probe's tool, policy and output.
pub(crate) fn kind(name: &'static str, wall_clock: Duration, max_tokens: u32) -> RunKind {
    RunKind {
        name,
        image: Image::Agent,
        limits: Limits {
            max_steps: 8,
            wall_clock,
            max_tokens,
        },
        manual: false,
        ..probe::kind()
    }
}

/// The app as one signed-in account and as sandboxes see it.
pub(crate) struct Harness {
    /// Signed in as `account`.
    app: Router,
    /// No session: what a sandbox reaches.
    agent: Router,
    pub(crate) db: PgPool,
    pub(crate) state: AppState,
    pub(crate) account: Uuid,
}

impl Harness {
    /// Composed over `deps`, signed in as a new account.
    pub(crate) fn with_deps(db: PgPool, deps: RunsDeps) -> Self {
        Self::as_account(db, deps, Uuid::now_v7())
    }

    /// Composed over `deps`, signed in as `account`.
    pub(crate) fn as_account(db: PgPool, deps: RunsDeps, account: Uuid) -> Self {
        let registry = applicamp_kernel::jobs::Registry::build(vec![(
            crate::NAME,
            module(RunsDeps::default()).background,
        )])
        .unwrap();
        let state = AppState::new(
            db.clone(),
            Config::for_database("postgres://unused"),
            SecretKey::from_bytes([7; 32]),
        )
        .with_events(EventBus::new(Arc::new(registry)));
        let (router, _) = module(deps).router.split_for_parts();
        let agent = router.clone().with_state(state.clone());
        let principal = Principal {
            account_id: account,
            session_id: Uuid::now_v7(),
        };
        let app = router
            .layer(middleware::from_fn(
                move |mut request: Request<Body>, next: Next| async move {
                    request.extensions_mut().insert(principal);
                    next.run(request).await
                },
            ))
            .with_state(state.clone());
        Self {
            app,
            agent,
            db,
            state,
            account,
        }
    }

    /// With runs on `runtime`, the agent API at a placeholder address.
    pub(crate) fn new(db: PgPool, runtime: Arc<dyn SandboxRuntime>) -> Self {
        let api = Url::parse("http://127.0.0.1:9").unwrap();
        Self::with_deps(db, RunsDeps::default().with_runtime(runtime, api))
    }

    async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// The raw answer to `request`, signed in.
    pub(crate) async fn raw(&self, request: Request<Body>) -> (StatusCode, String) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    pub(crate) async fn get(&self, uri: &str) -> (StatusCode, Value) {
        Self::call(&self.app, Request::get(uri).body(Body::empty()).unwrap()).await
    }

    pub(crate) async fn json(&self, method: &str, uri: &str, body: &Value) -> (StatusCode, Value) {
        Self::call(
            &self.app,
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    /// A call of the agent API with `token`.
    pub(crate) async fn agent(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<&Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(format!("/api/agent/v1/{path}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        };
        Self::call(&self.agent, request.unwrap()).await
    }

    /// `GET` of the agent API with `token` and a cookie too.
    pub(crate) async fn agent_with_cookie(&self, path: &str, token: &str) -> (StatusCode, Value) {
        let request = Request::get(format!("/api/agent/v1/{path}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::COOKIE, "__Host-applicamp_session=abc")
            .body(Body::empty())
            .unwrap();
        Self::call(&self.agent, request).await
    }

    /// The agent API's router, to serve on a port.
    pub(crate) fn agent_router(&self) -> Router {
        self.agent.clone()
    }

    /// A run of `kind` for `account`, going (as the orchestrator leaves it
    /// once its sandbox exists), with its token.
    pub(crate) async fn going(&self, account: Uuid, kind: &RunKind) -> (Uuid, String) {
        let now = Utc::now();
        let mut conn = self.db.acquire().await.unwrap();
        let id = store::queue(&mut conn, account, kind.name, None, &json!({}), now)
            .await
            .unwrap();
        drop(conn);
        sqlx::query(
            "UPDATE runs.runs SET status = 'starting', started_at = $2, claimed_by = 'test',
                    claimed_until = $2 + interval '1 hour' WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .execute(&self.db)
        .await
        .unwrap();
        let token = crypto::random_token();
        let tools: serde_json::Map<String, Value> = kind
            .tools
            .iter()
            .map(|tool| (tool.name.to_owned(), json!(tool.cap)))
            .collect();
        let deadline = now + chrono::TimeDelta::from_std(kind.limits.wall_clock).unwrap();
        let started = store::start(
            &self.db,
            id,
            "test",
            deadline,
            "agent",
            "policies/probe.yaml",
            &crypto::hash(&token),
            &Value::Object(tools),
            deadline + chrono::TimeDelta::minutes(5),
        )
        .await
        .unwrap();
        assert!(started);
        store::running(&self.db, id, "sandbox-test").await.unwrap();
        (id, token)
    }

    /// Run `id` as stored.
    pub(crate) async fn run(&self, id: Uuid) -> store::RunRow {
        store::fetch_any(&self.db, id).await.unwrap().unwrap()
    }

    /// Run `id`'s events, oldest first.
    pub(crate) async fn events(&self, id: Uuid) -> Vec<store::RunEvent> {
        store::events_after(&self.db, id, 0, None, 1_000)
            .await
            .unwrap()
    }

    /// The `runs.finished` events published, as `(run, outcome)`.
    pub(crate) async fn finished(&self) -> Vec<(Uuid, String)> {
        let rows: Vec<Value> = sqlx::query_scalar(
            "SELECT payload FROM kernel.events WHERE name = 'runs.finished' ORDER BY id",
        )
        .fetch_all(&self.db)
        .await
        .unwrap();
        rows.into_iter()
            .map(|payload| {
                (
                    payload["run"].as_str().unwrap().parse().unwrap(),
                    payload["outcome"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    /// Whether run `id` still has a token.
    pub(crate) async fn has_token(&self, id: Uuid) -> bool {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM runs.tokens WHERE run_id = $1)")
            .bind(id)
            .fetch_one(&self.db)
            .await
            .unwrap()
    }
}

/// Serve `router` on a free local port; its address.
pub(crate) async fn serve(router: Router) -> Url {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Url::parse(&format!("http://{addr}")).unwrap()
}

/// An orchestrator over `harness`'s database and `runtime`, the agent API
/// at `api`, executing `kinds` too, at most `max_runs` at once.
pub(crate) fn orchestrator(
    harness: &Harness,
    runtime: Arc<dyn SandboxRuntime>,
    api: Url,
    kinds: Vec<RunKind>,
    max_runs: u32,
) -> Orchestrator {
    let config = RunsConfig {
        max_runs: Some(max_runs),
        ..RunsConfig::default()
    };
    let deps = RunsDeps::new(config, None)
        .with_kinds(kinds)
        .with_runtime(runtime, api);
    Orchestrator::new(harness.db.clone(), harness.state.events.clone(), deps).unwrap()
}
