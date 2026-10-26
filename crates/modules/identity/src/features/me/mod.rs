//! `GET /api/v1/me`: who is signed in.

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::Provider;

/// The signed-in person.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    /// Their account.
    pub id: Uuid,
    /// Their name, as their first way in gave it.
    pub name: Option<String>,
    /// The verified address the account was created with.
    pub email: String,
    /// Their photo, if a provider gave one.
    pub picture_url: Option<String>,
    /// The providers that sign them in.
    pub ways_in: Vec<Provider>,
}

/// Who is signed in.
///
/// The account behind the session cookie, with the providers linked to it.
/// Never a token.
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "identity",
    operation_id = "identity.me",
    security(("session" = [])),
    responses(
        (status = 200, description = "The signed-in person.", body = Me),
        (status = 401, description = "`session_required`: no live session.", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn handler(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<Me>> {
    let account: Option<(Uuid, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, email, picture_url FROM identity.accounts WHERE id = $1")
            .bind(principal.account_id)
            .fetch_optional(&state.db)
            .await?;
    // A session whose account is gone is no session.
    let (id, name, email, picture_url) = account.ok_or(Error::Unauthorized(
        applicamp_kernel::auth::SESSION_REQUIRED,
    ))?;
    let providers: Vec<String> = sqlx::query_scalar(
        "SELECT provider FROM identity.ways_in WHERE account_id = $1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Me {
        id,
        name,
        email,
        picture_url,
        ways_in: providers
            .iter()
            .filter_map(|p| Provider::parse(p))
            .collect(),
    }))
}
