//! The module's routes over a real Postgres (`#[sqlx::test]`), signed in as
//! an invented account, and a fake OpenAI-compatible provider (wiremock).

use std::{sync::Arc, time::Duration};

use applicamp_ai::{AiConfig, Gateway, ProviderConfig, RetryPolicy};
use applicamp_kernel::{
    AppState, Config,
    auth::Principal,
    crypto::SecretKey,
    events::EventBus,
    jobs::{Registry, Worker},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
};
use http_body_util::BodyExt as _;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::{ProfileDeps, module};

/// The app as one signed-in account sees it.
pub(crate) struct Harness {
    app: Router,
    pub(crate) db: PgPool,
    pub(crate) account: Uuid,
}

/// One part of a `multipart/form-data` body.
pub(crate) enum Part<'a> {
    /// A text field.
    Text(&'a str, &'a str),
    /// A file field: name, file name, bytes.
    File(&'a str, &'a str, &'a [u8]),
}

const BOUNDARY: &str = "applicamp-test-boundary-7d1f";

fn multipart(parts: &[Part<'_>]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part {
            Part::Text(name, value) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            Part::File(name, file_name, bytes) => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; \
                         filename=\"{file_name}\"\r\n\
                         Content-Type: application/octet-stream\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(bytes);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

/// The module's background work, alone.
pub(crate) fn registry() -> Arc<Registry> {
    Arc::new(
        Registry::build(vec![(
            crate::NAME,
            module(ProfileDeps::default()).background,
        )])
        .unwrap(),
    )
}

impl Harness {
    /// Signed in as a new account, with `ai` (or none).
    pub(crate) fn new(db: PgPool, ai: Option<Gateway>) -> Self {
        Self::as_account(db, ai, Uuid::now_v7())
    }

    /// Signed in as `account`.
    pub(crate) fn as_account(db: PgPool, ai: Option<Gateway>, account: Uuid) -> Self {
        let events = EventBus::new(registry());
        let state = AppState::new(
            db.clone(),
            Config::for_database("postgres://unused"),
            SecretKey::from_bytes([7; 32]),
        )
        .with_events(events);
        let (router, _) = module(ProfileDeps { ai }).router.split_for_parts();
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
        Self { app, db, account }
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

    pub(crate) async fn post(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Request::post(uri).body(Body::empty()).unwrap())
            .await
    }

    /// `POST /api/v1/profile/imports` with `parts`.
    pub(crate) async fn upload(&self, parts: &[Part<'_>]) -> (StatusCode, Value) {
        self.send(
            Request::post("/api/v1/profile/imports")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(multipart(parts)))
                .unwrap(),
        )
        .await
    }

    /// Run every job that is due.
    pub(crate) async fn work(&self) {
        let registry = registry();
        let worker = Worker::new(
            self.db.clone(),
            Arc::clone(&registry),
            EventBus::new(registry),
        );
        while worker.run_once().await.unwrap() {}
    }

    pub(crate) async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.db).await.unwrap()
    }
}

/// A gateway to `server`'s fake model, capped at `daily_tokens`.
pub(crate) fn gateway(server: &MockServer, daily_tokens: Option<u64>) -> Gateway {
    Gateway::new(AiConfig {
        complete: Some(ProviderConfig::new(
            format!("{}/v1", server.uri()).parse().unwrap(),
            "test/model",
        )),
        daily_tokens,
        ..AiConfig::default()
    })
    .unwrap()
    .with_retry(RetryPolicy {
        attempts: 2,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(10),
    })
}

/// A fake model answering every chat completion with `answer` (as JSON
/// text in the message).
pub(crate) async fn fake_model(answer: &Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "test/model",
            "choices": [{
                "message": { "role": "assistant", "content": answer.to_string() },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 900, "completion_tokens": 300 },
        })))
        .mount(&server)
        .await;
    server
}
