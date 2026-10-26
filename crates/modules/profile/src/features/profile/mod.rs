//! `GET` and `PUT /api/v1/profile`: the signed-in person's profile, read
//! and edited by hand.

#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{Extension, Json, extract::State, extract::rejection::JsonRejection};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    events::ProfileUpdated,
    features::imports::{Ai, ImportKind},
    schema::Profile,
};

/// Where the profile came from last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    /// A LinkedIn data export.
    LinkedinZip,
    /// A PDF.
    Pdf,
    /// Pasted text.
    Text,
    /// Written in the app.
    Manual,
}

impl ProfileSource {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "linkedin_zip" => Some(Self::LinkedinZip),
            "pdf" => Some(Self::Pdf),
            "text" => Some(Self::Text),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// The profile, and what can be imported into it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProfileView {
    /// The profile; empty until something is imported or written.
    pub profile: Profile,
    /// Where it came from last (the last import applied, or `manual`).
    pub source: Option<ProfileSource>,
    /// When it last changed.
    pub updated_at: Option<DateTime<Utc>>,
    /// What this instance can import: always the LinkedIn export; PDFs and
    /// text only when a model is configured for them.
    pub import_kinds: Vec<ImportKind>,
}

/// The view of `profile`, last changed by `source` at `updated_at`.
pub(crate) fn view_of(
    profile: Profile,
    source: Option<&str>,
    updated_at: Option<DateTime<Utc>>,
    ai: &Ai,
) -> ProfileView {
    let mut import_kinds = vec![ImportKind::LinkedinZip];
    if ai.can_structure() {
        import_kinds.extend([ImportKind::Pdf, ImportKind::Text]);
    }
    ProfileView {
        profile,
        source: source.and_then(ProfileSource::parse),
        updated_at,
        import_kinds,
    }
}

/// The signed-in person's profile.
///
/// Empty (and no `source`) until they import or write one.
#[utoipa::path(
    get,
    path = "/api/v1/profile",
    tag = "profile",
    operation_id = "profile.get",
    security(("session" = [])),
    responses(
        (status = 200, description = "The profile.", body = ProfileView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
) -> Result<Json<ProfileView>> {
    let row: Option<(sqlx::types::Json<Profile>, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT data, source, updated_at FROM profile.profiles WHERE account_id = $1",
    )
    .bind(principal.account_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(match row {
        Some((profile, source, updated_at)) => {
            view_of(profile.0, Some(&source), Some(updated_at), &ai)
        }
        None => view_of(Profile::default(), None, None, &ai),
    }))
}

/// Save the profile.
///
/// The whole profile, as edited by hand. Values are trimmed and empty ones
/// removed; a value over its length, a date not like `2020`, `2020-03` or
/// `2020-03-15`, or a list of more than 200 entries is refused.
#[utoipa::path(
    put,
    path = "/api/v1/profile",
    tag = "profile",
    operation_id = "profile.update",
    security(("session" = [])),
    request_body = Profile,
    responses(
        (status = 200, description = "The profile, saved.", body = ProfileView),
        (status = 400, description = "`invalid_profile`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    body: std::result::Result<Json<Profile>, JsonRejection>,
) -> Result<Json<ProfileView>> {
    let Json(profile) =
        body.map_err(|rejection| Error::BadRequest("invalid_profile", rejection.body_text()))?;
    let profile = profile.tidied();
    profile.validate()?;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let source: String = sqlx::query_scalar(
        "INSERT INTO profile.profiles (account_id, data, source, updated_at)
         VALUES ($1, $2, 'manual', $3)
         ON CONFLICT (account_id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at
         RETURNING source",
    )
    .bind(principal.account_id)
    .bind(sqlx::types::Json(&profile))
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    state
        .events
        .publish(
            &mut tx,
            &ProfileUpdated {
                account: principal.account_id,
            },
        )
        .await?;
    tx.commit().await?;
    Ok(Json(view_of(profile, Some(&source), Some(now), &ai)))
}
