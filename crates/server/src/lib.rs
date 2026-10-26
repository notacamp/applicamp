//! `applicamp-server`: composes the modules into one HTTP application.
//!
//! This is the only crate that knows the full list of modules. Adding a
//! module means adding one line to [`modules`]. Their background work is
//! collected by [`registry`]; their routes, the BFF's and `/healthz` form
//! the OpenAPI document ([`openapi`]), and every route is declared through
//! `utoipa-axum` so it cannot be missing from it (`cargo xtask arch`).

#![forbid(unsafe_code)]

pub mod bff;

use std::{sync::Arc, time::Instant};

use applicamp_kernel::{AppState, Error, Module, Problem, Result, jobs::Registry};
use axum::{
    Json, Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::{self, Next},
    response::Response,
};
use serde::Serialize;
use sqlx::PgPool;
use tower_http::set_header::SetResponseHeaderLayer;
use utoipa::{OpenApi as _, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

/// The server's version (`VERSION`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Path of the health check, which `applicamp healthcheck` probes.
pub const HEALTH_PATH: &str = "/healthz";

/// Response headers on every answer.
const SECURITY_HEADERS: [(HeaderName, &str); 3] = [
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::X_FRAME_OPTIONS, "DENY"),
];

/// Every module, in the order their routes are mounted.
pub fn modules() -> Vec<Module> {
    vec![applicamp_identity::module()]
}

/// Every module's background work.
pub fn registry() -> Result<Registry, Vec<String>> {
    Registry::build(
        modules()
            .into_iter()
            .map(|module| (module.name, module.background))
            .collect(),
    )
}

/// Apply the migrations in `migrations/` (embedded at build time).
pub async fn migrate(db: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(db).await
}

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Applicamp API",
        description = "The Applicamp HTTP API. Errors are RFC 9457 \
            `application/problem+json` with a stable `code`. Operations either are public or \
            take the web app's session cookie (`session`); unsafe methods also need the \
            `X-Applicamp-CSRF: 1` header and the app's own `Origin`.",
        license(name = "Sustainable Use License", identifier = "LicenseRef-Sustainable-Use"),
    ),
    components(schemas(Problem)),
    tags(
        (name = "system", description = "Instance health and version."),
        (name = "identity", description = "Accounts and their ways in (Campbooks, LinkedIn). \
            Who may create an account is the invite list, managed from the server's command \
            line only."),
        (name = "bff", description = "The web app's backend-for-frontend: sign-in with a \
            provider, the session cookie, sign-out. Browsers only."),
    ),
)]
struct ApiDoc;

/// The module routes and `/healthz`, with their docs.
fn api() -> OpenApiRouter<AppState> {
    let mut doc = ApiDoc::openapi();
    VERSION.clone_into(&mut doc.info.version);
    doc.components
        .get_or_insert_with(Default::default)
        .add_security_scheme(bff::SESSION_SCHEME, bff::security_scheme());
    modules()
        .into_iter()
        .fold(OpenApiRouter::with_openapi(doc), |api, module| {
            api.merge(module.router)
        })
        .routes(routes!(health))
}

/// The OpenAPI document: every route the server answers.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = api().into_openapi();
    doc.merge(bff::router().into_openapi());
    doc
}

/// The whole HTTP application over `state`: module routes (behind the
/// session lookup), the BFF, a problem+json 404 fallback, and the
/// middleware every request passes (CSRF check, security headers,
/// `no-store` on the API, a log line per request).
pub fn app(state: AppState, bff: &bff::Bff) -> Router {
    let (api, _) = api().split_for_parts();
    let api = api
        .layer(middleware::from_fn_with_state(
            bff.clone(),
            bff::authenticate,
        ))
        .with_state(state);
    let (bff_routes, _) = bff::router().split_for_parts();
    let [nosniff, referrer, frame] = SECURITY_HEADERS.map(|(name, value)| {
        SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
    });
    api.merge(bff_routes.with_state(bff.clone()))
        .fallback(|| async { Error::NotFound })
        .layer(middleware::from_fn_with_state(
            bff.public_origin(),
            bff::csrf::check,
        ))
        .layer(middleware::from_fn(no_store))
        .layer(nosniff)
        .layer(referrer)
        .layer(frame)
        .layer(middleware::from_fn(log_request))
}

/// The instance's health.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// `ok`.
    pub status: &'static str,
    /// The server's version.
    pub version: &'static str,
}

/// Health check.
///
/// `ok` when the server answers and reaches its database.
#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    operation_id = "system.health",
    security(()),
    responses(
        (status = 200, description = "Healthy.", body = Health),
        (status = 503, description = "`database_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn health(State(state): State<AppState>) -> Result<Json<Health>> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| Error::Unavailable("database_unavailable"))?;
    Ok(Json(Health {
        status: "ok",
        version: VERSION,
    }))
}

/// Answers of the API and the BFF are never cached.
async fn no_store(request: Request, next: Next) -> Response {
    let api = {
        let path = request.uri().path();
        path.starts_with("/api/") || path.starts_with("/bff/")
    };
    let mut response = next.run(request).await;
    if api {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// One log line per request: method, route template, status and duration.
/// Never the URI: a query string can hold a code or a token.
async fn log_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route: Arc<str> = request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".into(), |path| path.as_str().into());
    let started = Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        %method,
        route = %route,
        status = response.status().as_u16(),
        duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "request"
    );
    response
}
