//! The module's routes over a real Postgres (`#[sqlx::test]`), signed in as
//! an invented account, with fake neighbours.

use std::sync::{Arc, Mutex, PoisonError};

use applicamp_kernel::{
    AppState, BoxFuture, Config, Result, auth::Principal, crypto::SecretKey, events::EventBus,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt as _;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt as _;
use url::Url;
use uuid::Uuid;

use crate::{
    PipelineDeps, module,
    ports::{
        AnswerFacts, CvProfile, Neighbours, Posting, ProfileDigest, ProfileStatus, SearchLine,
        SiteFailure, SiteReading,
    },
};

/// Neighbours that answer what a test sets.
#[derive(Default)]
pub(crate) struct Fake {
    pub(crate) posting: Mutex<Option<Posting>>,
    pub(crate) status: Mutex<ProfileStatus>,
    pub(crate) latest: Mutex<Option<SearchLine>>,
    /// The links it was asked to read.
    pub(crate) read: Mutex<Vec<String>>,
    /// What reading a company's site comes to; unreachable until a test sets
    /// it.
    pub(crate) site: Mutex<Option<std::result::Result<SiteReading, SiteFailure>>>,
    /// The sites (and postings) it was asked to read.
    pub(crate) sites_read: Mutex<Vec<(String, Option<String>)>>,
    /// The profile as a job is read against it.
    pub(crate) digest: Mutex<Option<ProfileDigest>>,
    /// What an answer about a job comes to, and the positions and answers it
    /// was asked about.
    pub(crate) answered: Mutex<Option<AnswerFacts>>,
    pub(crate) answers: Mutex<Vec<(usize, String)>>,
    /// The skills said not to be the person's.
    pub(crate) dismissed: Mutex<Vec<String>>,
    /// Hosts the guard refuses (as `localhost` is on a real instance).
    pub(crate) refused: Mutex<Vec<String>>,
    /// The whole profile, as a tailored CV is made from it.
    pub(crate) cv_profile: Mutex<Option<CvProfile>>,
    /// The facts kept from a CV: `(position, text, area)`.
    pub(crate) kept: Mutex<Vec<(usize, String, String)>>,
    /// The skills added from a CV.
    pub(crate) added_skills: Mutex<Vec<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Neighbours for Fake {
    fn read_posting<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
        url: &'a Url,
    ) -> BoxFuture<'a, Result<Option<Posting>>> {
        lock(&self.read).push(url.to_string());
        let posting = lock(&self.posting).clone();
        // Only the board's own links are postings, like the real reader.
        let posting = posting.filter(|_| url.host_str() == Some("boards.greenhouse.io"));
        Box::pin(async move { Ok(posting) })
    }

    fn profile_status<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
    ) -> BoxFuture<'a, Result<ProfileStatus>> {
        let status = lock(&self.status).clone();
        Box::pin(async move { Ok(status) })
    }

    fn latest_search<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
    ) -> BoxFuture<'a, Result<Option<SearchLine>>> {
        let latest = lock(&self.latest).clone();
        Box::pin(async move { Ok(latest) })
    }

    fn site_allowed(&self, site: &Url) -> bool {
        let host = site.host_str().unwrap_or_default().to_owned();
        !lock(&self.refused).contains(&host)
    }

    fn read_site<'a>(
        &'a self,
        site: &'a Url,
        posting: Option<&'a Url>,
        _now: DateTime<Utc>,
    ) -> BoxFuture<'a, std::result::Result<SiteReading, SiteFailure>> {
        lock(&self.sites_read).push((site.to_string(), posting.map(Url::to_string)));
        let reading = lock(&self.site)
            .clone()
            .unwrap_or(Err(SiteFailure::Unreachable));
        Box::pin(async move { reading })
    }

    fn profile_digest<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
    ) -> BoxFuture<'a, Result<Option<ProfileDigest>>> {
        let digest = lock(&self.digest).clone();
        Box::pin(async move { Ok(digest) })
    }

    fn answer_about<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
        _now: DateTime<Utc>,
        position: usize,
        _question: &'a str,
        answer: &'a str,
        _locale: Option<&'a str>,
    ) -> BoxFuture<'a, Result<AnswerFacts>> {
        lock(&self.answers).push((position, answer.to_owned()));
        let answered = lock(&self.answered).clone();
        Box::pin(async move {
            answered.ok_or_else(|| applicamp_kernel::Error::Unavailable("ai_unavailable"))
        })
    }

    fn dismiss_skill<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
        skill: &'a str,
        _now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<()>> {
        lock(&self.dismissed).push(skill.to_lowercase());
        Box::pin(async move { Ok(()) })
    }

    fn cv_profile<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
    ) -> BoxFuture<'a, Result<Option<CvProfile>>> {
        let profile = lock(&self.cv_profile).clone();
        Box::pin(async move { Ok(profile) })
    }

    fn keep_fact<'a>(
        &'a self,
        _state: &'a AppState,
        _account: Uuid,
        position: usize,
        _title: Option<&'a str>,
        _company: Option<&'a str>,
        text: &'a str,
        area: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        lock(&self.kept).push((position, text.to_owned(), area.to_owned()));
        Box::pin(async move { Ok(Uuid::now_v7().to_string()) })
    }

    fn add_skill<'a>(
        &'a self,
        _state: &'a AppState,
        _account: Uuid,
        skill: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        lock(&self.added_skills).push(skill.to_owned());
        Box::pin(async move { Ok(()) })
    }
}

impl Fake {
    /// Reading a company's site answers `reading`.
    pub(crate) fn reads(&self, reading: SiteReading) {
        *lock(&self.site) = Some(Ok(reading));
    }
}

/// The app as one signed-in account sees it.
pub(crate) struct Harness {
    app: Router,
    pub(crate) db: PgPool,
    pub(crate) account: Uuid,
    pub(crate) fake: Arc<Fake>,
}

impl Harness {
    pub(crate) fn new(db: PgPool) -> Self {
        Self::as_account(db, Uuid::now_v7(), Arc::new(Fake::default()))
    }

    pub(crate) fn as_account(db: PgPool, account: Uuid, fake: Arc<Fake>) -> Self {
        Self::with_ai(db, account, fake, None)
    }

    /// Signed in as `account`, with the model `ai` (or none) composed in.
    pub(crate) fn with_ai(
        db: PgPool,
        account: Uuid,
        fake: Arc<Fake>,
        ai: Option<applicamp_ai::Gateway>,
    ) -> Self {
        let state = AppState::new(
            db.clone(),
            Config::for_database("postgres://unused"),
            SecretKey::from_bytes([7; 32]),
        )
        .with_events(EventBus::default());
        let neighbours: Arc<dyn Neighbours> = fake.clone();
        let (router, _) = module(PipelineDeps {
            neighbours: Some(neighbours),
            ai,
        })
        .router
        .split_for_parts();
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
            .with_state(state);
        Self {
            app,
            db,
            account,
            fake,
        }
    }

    pub(crate) async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub(crate) async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    /// A GET whose answer is not JSON: the status, headers and body bytes.
    pub(crate) async fn get_raw(&self, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let response = self
            .app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes.to_vec())
    }

    pub(crate) async fn send_delete(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Request::delete(uri).body(Body::empty()).unwrap())
            .await
    }

    pub(crate) async fn json(&self, method: &str, uri: &str, body: &Value) -> (StatusCode, Value) {
        self.send(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }
}

/// A fake OpenAI-compatible model that answers the brief's two prompts by
/// what the system message says, and counts the calls of each.
pub(crate) struct Model {
    pub(crate) server: wiremock::MockServer,
}

struct Routed {
    facts: Mutex<Value>,
    analysis: Mutex<Value>,
    cv: Mutex<Value>,
}

impl wiremock::Respond for Routed {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let system = body["messages"][0]["content"].as_str().unwrap_or_default();
        let answer = if system.starts_with("You read pages from a company") {
            lock(&self.facts).clone()
        } else if system.starts_with(crate::features::cv::prompt::PREFIX) {
            lock(&self.cv).clone()
        } else {
            lock(&self.analysis).clone()
        };
        wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "test/model",
            "choices": [{
                "message": { "role": "assistant", "content": answer.to_string() },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 900, "completion_tokens": 300 },
        }))
    }
}

impl Model {
    /// A model that says `facts` to the company prompt and `analysis` to the
    /// job prompt.
    pub(crate) async fn new(facts: Value, analysis: Value) -> Self {
        Self::with_cv(facts, analysis, Value::Null).await
    }

    /// A model that also answers the CV prompt with `cv`.
    pub(crate) async fn with_cv(facts: Value, analysis: Value, cv: Value) -> Self {
        // Its own server, not one from wiremock's pool, which is shared across the
        // tests' runtimes: the calls counted are this test's alone.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let server = wiremock::MockServer::builder()
            .listener(listener)
            .start()
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(Routed {
                facts: Mutex::new(facts),
                analysis: Mutex::new(analysis),
                cv: Mutex::new(cv),
            })
            .mount(&server)
            .await;
        Self { server }
    }

    /// The gateway to it.
    pub(crate) fn gateway(&self) -> applicamp_ai::Gateway {
        use applicamp_ai::{AiConfig, ProviderConfig, RetryPolicy};
        applicamp_ai::Gateway::new(AiConfig {
            complete: Some(
                ProviderConfig::new(
                    format!("{}/v1", self.server.uri()).parse().unwrap(),
                    "test/model",
                )
                .with_price(0.15, 0.5),
            ),
            ..AiConfig::default()
        })
        .unwrap()
        .with_retry(RetryPolicy {
            attempts: 2,
            base_delay: std::time::Duration::from_millis(5),
            max_delay: std::time::Duration::from_millis(10),
        })
    }

    /// The user messages of the calls made so far whose system message
    /// starts with `prefix`.
    pub(crate) async fn user_messages(&self, prefix: &str) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter_map(|request| serde_json::from_slice::<Value>(&request.body).ok())
            .filter(|body| {
                body["messages"][0]["content"]
                    .as_str()
                    .is_some_and(|system| system.starts_with(prefix))
            })
            .filter_map(|body| body["messages"][1]["content"].as_str().map(str::to_owned))
            .collect()
    }

    /// The calls made so far whose system message starts with `prefix`.
    pub(crate) async fn calls(&self, prefix: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| {
                serde_json::from_slice::<Value>(&request.body)
                    .ok()
                    .and_then(|b| b["messages"][0]["content"].as_str().map(str::to_owned))
                    .is_some_and(|system| system.starts_with(prefix))
            })
            .count()
    }
}
