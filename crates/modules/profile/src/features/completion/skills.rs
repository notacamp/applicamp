//! The order of the skills, which matters: the first three drive the job
//! board searches. (Suggesting, adding and dismissing found skills was
//! removed in #49.)

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::write_profile;
use crate::{
    features::{
        imports::Ai,
        profile::{ProfileView, view_of},
    },
    schema::{Profile, Skill},
};

/// The locked profile of `account`: data and source.
async fn lock_profile(tx: &mut sqlx::PgConnection, account: Uuid) -> Result<(Profile, String)> {
    let row: Option<(sqlx::types::Json<Profile>, String)> = sqlx::query_as(
        "SELECT data, source FROM profile.profiles WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?;
    let (profile, source) = row.ok_or(Error::NotFound)?;
    Ok((profile.0, source))
}

/// The skills in the order the person wants them.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SkillOrder {
    /// Every skill name, once, in the new order.
    pub names: Vec<String>,
}

/// Put the skills in a new order.
///
/// The first three skills drive the job board searches. `names` must be
/// the profile's skills exactly, each once; nothing is added or removed.
#[utoipa::path(
    put,
    path = "/api/v1/profile/skills/order",
    tag = "profile",
    operation_id = "profile.order_skills",
    security(("session" = [])),
    request_body = SkillOrder,
    responses(
        (status = 200, description = "The profile, with its skills in order.", body = ProfileView),
        (status = 400, description = "`invalid_order`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn order_skills(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    body: std::result::Result<Json<SkillOrder>, JsonRejection>,
) -> Result<Json<ProfileView>> {
    let Json(SkillOrder { names }) =
        body.map_err(|rejection| Error::BadRequest("invalid_order", rejection.body_text()))?;
    let account = principal.account_id;
    let mut tx = state.db.begin().await?;
    let (mut profile, source) = lock_profile(&mut tx, account).await?;
    let mut rest = std::mem::take(&mut profile.skills);
    let mut ordered: Vec<Skill> = Vec::with_capacity(rest.len());
    for name in &names {
        let at = rest.iter().position(|s| s.name == *name).ok_or_else(|| {
            Error::BadRequest(
                "invalid_order",
                "The names must be the profile's skills, each once.".to_owned(),
            )
        })?;
        ordered.push(rest.remove(at));
    }
    if !rest.is_empty() {
        return Err(Error::BadRequest(
            "invalid_order",
            "The names must be the profile's skills, each once.".to_owned(),
        ));
    }
    profile.skills = ordered;
    let now = state.clock.now();
    write_profile(&state, &mut tx, account, &profile, None).await?;
    tx.commit().await?;
    Ok(Json(view_of(profile, Some(&source), Some(now), &ai)))
}
