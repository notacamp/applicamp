//! `/.well-known/openid-configuration` and `/oauth2/jwks`: what the
//! OpenShell gateway reads to trust this server's own tokens
//! ([`crate::issuer`]). Served only with the OpenShell runtime; nothing is
//! issued here.

use applicamp_kernel::{Error, Problem, Result};
use axum::{Extension, Json};
use serde_json::Value;

use crate::RunsDeps;

/// The issuer's discovery document.
///
/// The issuer (this instance's public origin) and where its keys are. Only
/// when sandboxes run on OpenShell; not found otherwise.
#[utoipa::path(
    get,
    path = "/.well-known/openid-configuration",
    tag = "runs",
    operation_id = "runs.oidc_discovery",
    security(()),
    responses(
        (status = 200, description = "The discovery document.", body = Value),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn discovery(Extension(deps): Extension<RunsDeps>) -> Result<Json<Value>> {
    let issuer = deps.issuer.as_ref().ok_or(Error::NotFound)?;
    Ok(Json(issuer.discovery()))
}

/// The issuer's keys.
///
/// One Ed25519 key (JWK, `OKP`), derived from the instance secret for the
/// gateway only. Only when sandboxes run on OpenShell; not found otherwise.
#[utoipa::path(
    get,
    path = "/oauth2/jwks",
    tag = "runs",
    operation_id = "runs.oidc_jwks",
    security(()),
    responses(
        (status = 200, description = "The key set.", body = Value),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn jwks(Extension(deps): Extension<RunsDeps>) -> Result<Json<Value>> {
    let issuer = deps.issuer.as_ref().ok_or(Error::NotFound)?;
    Ok(Json(issuer.jwks()))
}
