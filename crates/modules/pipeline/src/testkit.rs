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
use http_body_util::BodyExt as _;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt as _;
use url::Url;
use uuid::Uuid;

use crate::{
    PipelineDeps, module,
    ports::{Neighbours, Posting, ProfileStatus, SearchLine},
};

/// Neighbours that answer what a test sets.
#[derive(Default)]
pub(crate) struct Fake {
    pub(crate) posting: Mutex<Option<Posting>>,
    pub(crate) status: Mutex<ProfileStatus>,
    pub(crate) latest: Mutex<Option<SearchLine>>,
    /// The links it was asked to read.
    pub(crate) read: Mutex<Vec<String>>,
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
        let state = AppState::new(
            db.clone(),
            Config::for_database("postgres://unused"),
            SecretKey::from_bytes([7; 32]),
        )
        .with_events(EventBus::default());
        let neighbours: Arc<dyn Neighbours> = fake.clone();
        let (router, _) = module(PipelineDeps {
            neighbours: Some(neighbours),
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
