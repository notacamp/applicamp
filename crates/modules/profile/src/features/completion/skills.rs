//! Skills found in the person's own positions: suggested from the review
//! (checked again against the profile as it is now), added with the
//! person's click, or marked not theirs. And the order of the skills, which
//! matters: the first three drive the job board searches.

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    Review, ReviewRow, ReviewStatus, carry_forward, current, ensure, review, stored, write_profile,
};
use crate::{
    features::{
        imports::Ai,
        profile::{ProfileView, view_of},
    },
    schema::{Profile, Skill},
};

/// A skill the person's positions mention and their skills list does not.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SuggestedSkill {
    /// The skill, as the positions write it.
    pub name: String,
    /// In how many positions it appears.
    pub roles: u32,
}

/// Skills found in the person's roles.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SuggestedSkills {
    /// Where the review is (the same review as the questions').
    pub status: ReviewStatus,
    /// Why there are none: `ai_unavailable`, `ai_budget_reached`,
    /// `ai_daily_limit_reached` or `ai_failed`.
    pub reason: Option<String>,
    /// The skills, the most roles first.
    pub skills: Vec<SuggestedSkill>,
}

/// The review's skills as they are now: not listed, not dismissed, and still
/// in at least one position as the profile stands.
pub(crate) async fn visible(
    db: &PgPool,
    account: Uuid,
    profile: &Profile,
    row: &ReviewRow,
) -> Result<Vec<SuggestedSkill>> {
    let dismissed: Vec<String> =
        sqlx::query_scalar("SELECT name_key FROM profile.skill_dismissals WHERE account_id = $1")
            .bind(account)
            .fetch_all(db)
            .await?;
    Ok(suggestions(profile, row, &dismissed))
}

fn suggestions(profile: &Profile, row: &ReviewRow, dismissed: &[String]) -> Vec<SuggestedSkill> {
    let mut found: Vec<SuggestedSkill> = row
        .skills
        .0
        .iter()
        .filter(|skill| {
            let key = skill.name.to_lowercase();
            !dismissed.contains(&key)
                && !profile.skills.iter().any(|s| s.name.to_lowercase() == key)
        })
        .filter_map(|skill| {
            let roles = review::roles_mentioning(profile, &skill.name);
            (!roles.is_empty()).then(|| SuggestedSkill {
                name: skill.name.clone(),
                roles: u32::try_from(roles.len()).unwrap_or(u32::MAX),
            })
        })
        .collect();
    found.sort_by_key(|skill| std::cmp::Reverse(skill.roles));
    found
}

async fn suggested_of(
    state: &AppState,
    ai: &Ai,
    account: Uuid,
) -> Result<(StatusCode, Json<SuggestedSkills>)> {
    let (current, review) = ensure(state, ai, account, false).await?;
    let none = |status, reason: Option<String>| SuggestedSkills {
        status,
        reason,
        skills: Vec::new(),
    };
    Ok(match (current, review) {
        (_, Review::Empty) => (StatusCode::OK, Json(none(ReviewStatus::Unavailable, None))),
        (_, Review::Unavailable(reason)) => (
            StatusCode::OK,
            Json(none(ReviewStatus::Unavailable, Some(reason.to_owned()))),
        ),
        (_, Review::Running) => (
            StatusCode::ACCEPTED,
            Json(none(ReviewStatus::Running, None)),
        ),
        (_, Review::Failed(reason)) => (
            StatusCode::OK,
            Json(none(ReviewStatus::Failed, Some(reason))),
        ),
        (Some(current), Review::Ready(row)) => (
            StatusCode::OK,
            Json(SuggestedSkills {
                status: ReviewStatus::Ready,
                reason: None,
                skills: visible(&state.db, account, &current.profile, &row).await?,
            }),
        ),
        (None, Review::Ready(_)) => (StatusCode::OK, Json(none(ReviewStatus::Unavailable, None))),
    })
}

/// Skills found in the roles.
///
/// Skills that the person's own position descriptions mention but their
/// skills list does not, with the number of positions each is in. Each
/// name is checked against the text of the positions (whole words), so
/// nothing is suggested that the profile does not say. Uses the same
/// review as the questions; 202 while it runs.
#[utoipa::path(
    get,
    path = "/api/v1/profile/skills/suggested",
    tag = "profile",
    operation_id = "profile.suggested_skills",
    security(("session" = [])),
    responses(
        (status = 200, description = "The skills.", body = SuggestedSkills),
        (status = 202, description = "The review is running.", body = SuggestedSkills),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn suggested_skills(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
) -> Result<(StatusCode, Json<SuggestedSkills>)> {
    suggested_of(&state, &ai, principal.account_id).await
}

/// The locked profile of `account`: data, version and source.
async fn lock_profile(
    tx: &mut sqlx::PgConnection,
    account: Uuid,
) -> Result<(Profile, i32, String)> {
    let row: Option<(sqlx::types::Json<Profile>, i32, String)> = sqlx::query_as(
        "SELECT data, version, source FROM profile.profiles WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?;
    let (profile, version, source) = row.ok_or(Error::NotFound)?;
    Ok((profile.0, version, source))
}

/// Add a suggested skill to the profile.
///
/// It goes at the end of the skills, in the order the person adds them.
#[utoipa::path(
    post,
    path = "/api/v1/profile/skills/suggested/{name}/add",
    tag = "profile",
    operation_id = "profile.add_suggested_skill",
    security(("session" = [])),
    params(("name" = String, Path, description = "The skill, as suggested.")),
    responses(
        (status = 200, description = "The skills still suggested.", body = SuggestedSkills),
        (status = 400, description = "`invalid_profile`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: not a suggestion", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn add_skill(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(name): Path<String>,
) -> Result<Json<SuggestedSkills>> {
    let account = principal.account_id;
    let seen = current(&state.db, account).await?.ok_or(Error::NotFound)?;
    let row = stored(&state.db, account, &seen)
        .await?
        .filter(|row| row.status == "ready")
        .ok_or(Error::NotFound)?;
    let mut tx = state.db.begin().await?;
    let (mut profile, version, _) = lock_profile(&mut tx, account).await?;
    let dismissed: Vec<String> =
        sqlx::query_scalar("SELECT name_key FROM profile.skill_dismissals WHERE account_id = $1")
            .bind(account)
            .fetch_all(&mut *tx)
            .await?;
    let key = name.trim().to_lowercase();
    let suggestion = suggestions(&profile, &row, &dismissed)
        .into_iter()
        .find(|s| s.name.to_lowercase() == key)
        .ok_or(Error::NotFound)?;
    profile.skills.push(Skill {
        name: suggestion.name,
        level: None,
    });
    profile.validate()?;
    let new_version = write_profile(&state, &mut tx, account, &profile, None).await?;
    carry_forward(&mut tx, account, version, new_version, None).await?;
    tx.commit().await?;
    let left = suggestions(&profile, &row, &dismissed);
    Ok(Json(SuggestedSkills {
        status: ReviewStatus::Ready,
        reason: None,
        skills: left,
    }))
}

/// Say a suggested skill is not theirs.
///
/// It is not suggested again, whatever the profile says later.
#[utoipa::path(
    post,
    path = "/api/v1/profile/skills/suggested/{name}/dismiss",
    tag = "profile",
    operation_id = "profile.dismiss_suggested_skill",
    security(("session" = [])),
    params(("name" = String, Path, description = "The skill, as suggested.")),
    responses(
        (status = 200, description = "The skills still suggested.", body = SuggestedSkills),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: not a suggestion", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn dismiss_skill(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(name): Path<String>,
) -> Result<Json<SuggestedSkills>> {
    let account = principal.account_id;
    let seen = current(&state.db, account).await?.ok_or(Error::NotFound)?;
    let row = stored(&state.db, account, &seen)
        .await?
        .filter(|row| row.status == "ready")
        .ok_or(Error::NotFound)?;
    let key = name.trim().to_lowercase();
    let shown = visible(&state.db, account, &seen.profile, &row).await?;
    if !shown.iter().any(|s| s.name.to_lowercase() == key) {
        return Err(Error::NotFound);
    }
    sqlx::query(
        "INSERT INTO profile.skill_dismissals (account_id, name_key, created_at)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(account)
    .bind(&key)
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    let skills = visible(&state.db, account, &seen.profile, &row).await?;
    Ok(Json(SuggestedSkills {
        status: ReviewStatus::Ready,
        reason: None,
        skills,
    }))
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
    let (mut profile, version, source) = lock_profile(&mut tx, account).await?;
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
    let new_version = write_profile(&state, &mut tx, account, &profile, None).await?;
    carry_forward(&mut tx, account, version, new_version, None).await?;
    tx.commit().await?;
    Ok(Json(view_of(profile, Some(&source), Some(now), &ai)))
}
