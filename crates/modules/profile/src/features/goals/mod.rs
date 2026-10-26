//! `GET` and `PUT /api/v1/goals`: the signed-in person's goals, answered one
//! question at a time and edited by hand. Every save is a new version.

#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{Json, extract::State, extract::rejection::JsonRejection};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{events::GoalsUpdated, goals::Goals};

/// The goals and the version they are at.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoalsView {
    /// 0 until something is saved; each save adds one.
    pub version: i32,
    /// The answers; every one optional.
    pub answers: Goals,
    /// When they last changed.
    pub updated_at: Option<DateTime<Utc>>,
}

/// The signed-in person's goals.
///
/// Version 0 and no answers until they answer a question.
#[utoipa::path(
    get,
    path = "/api/v1/goals",
    tag = "profile",
    operation_id = "profile.get_goals",
    security(("session" = [])),
    responses(
        (status = 200, description = "The goals.", body = GoalsView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<GoalsView>> {
    let row: Option<(i32, sqlx::types::Json<Goals>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT version, answers, updated_at FROM profile.goals WHERE account_id = $1",
    )
    .bind(principal.account_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(match row {
        Some((version, answers, updated_at)) => GoalsView {
            version,
            answers: answers.0,
            updated_at: Some(updated_at),
        },
        None => GoalsView {
            version: 0,
            answers: Goals::default(),
            updated_at: None,
        },
    }))
}

/// Save the goals.
///
/// All the answers at once, as the interview or an edit leaves them; a
/// question left out is unanswered. Values are trimmed and blank ones
/// removed. Lists over their limit (5 roles; 10 places, things to avoid and
/// must-haves), a currency that is not ISO 4217 or a choice that does not
/// exist are refused. Every save, even of the same answers, is a new version.
#[utoipa::path(
    put,
    path = "/api/v1/goals",
    tag = "profile",
    operation_id = "profile.update_goals",
    security(("session" = [])),
    request_body = Goals,
    responses(
        (status = 200, description = "The goals, saved, at their new version.", body = GoalsView),
        (status = 400, description = "`invalid_goals`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    body: std::result::Result<Json<Goals>, JsonRejection>,
) -> Result<Json<GoalsView>> {
    let Json(answers) =
        body.map_err(|rejection| Error::BadRequest("invalid_goals", rejection.body_text()))?;
    let answers = answers.tidied();
    answers.validate()?;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let version: i32 = sqlx::query_scalar(
        "INSERT INTO profile.goals (account_id, version, answers, updated_at)
         VALUES ($1, 1, $2, $3)
         ON CONFLICT (account_id) DO UPDATE
         SET version = goals.version + 1, answers = excluded.answers,
             updated_at = excluded.updated_at
         RETURNING version",
    )
    .bind(principal.account_id)
    .bind(sqlx::types::Json(&answers))
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    state
        .events
        .publish(
            &mut tx,
            &GoalsUpdated {
                account: principal.account_id,
                version,
            },
        )
        .await?;
    tx.commit().await?;
    Ok(Json(GoalsView {
        version,
        answers,
        updated_at: Some(now),
    }))
}
