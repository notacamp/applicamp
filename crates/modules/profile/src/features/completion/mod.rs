//! Profile completion: how strong the profile is, and the two helpers the
//! other slices write the profile with.
//!
//! The AI questions, drafts and found skills that used to live here were
//! retired from the client in #44 and removed in #49: Tell me about you
//! ([`super::about`]) asks about the gaps now.

pub(crate) mod check;
pub mod skills;
#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Problem, Result, auth::SignedIn};
use axum::{Json, extract::State};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    events::ProfileUpdated,
    goals::Goals,
    schema::Profile,
    strength::{self, StrengthKey, StrengthState as PartState},
};

/// The profile and goals of an account, as they stand.
pub(crate) struct Current {
    pub(crate) profile: Profile,
    pub(crate) version: i32,
    pub(crate) goals: Goals,
    /// Nothing reads it since the gap review went (#49); kept as it is, as
    /// Tell me about you shares this type.
    #[allow(dead_code)]
    pub(crate) goals_version: i32,
}

/// The profile and goals of `account`; none until there is a profile.
pub(crate) async fn current(db: &PgPool, account: Uuid) -> Result<Option<Current>> {
    let row: Option<(sqlx::types::Json<Profile>, i32)> =
        sqlx::query_as("SELECT data, version FROM profile.profiles WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    let Some((profile, version)) = row else {
        return Ok(None);
    };
    let goals: Option<(sqlx::types::Json<Goals>, i32)> =
        sqlx::query_as("SELECT answers, version FROM profile.goals WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    let (goals, goals_version) = goals.map_or((Goals::default(), 0), |(g, v)| (g.0, v));
    Ok(Some(Current {
        profile: profile.0,
        version,
        goals,
        goals_version,
    }))
}

/// Write `profile` as the account's profile, inside `tx`: the version goes up
/// by one (a trigger). `source` replaces where it came from when given.
/// Publishes `profile.updated`. The new version.
pub(crate) async fn write_profile(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    account: Uuid,
    profile: &Profile,
    source: Option<&str>,
) -> Result<i32> {
    let version: i32 = sqlx::query_scalar(
        "UPDATE profile.profiles
            SET data = $2, source = COALESCE($3, source), updated_at = $4
          WHERE account_id = $1 RETURNING version",
    )
    .bind(account)
    .bind(sqlx::types::Json(profile))
    .bind(source)
    .bind(state.clock.now())
    .fetch_one(&mut *tx)
    .await?;
    state
        .events
        .publish(tx, &ProfileUpdated { account })
        .await?;
    Ok(version)
}

// ---- strength -------------------------------------------------------------

/// One part of the score.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StrengthItem {
    /// Which part.
    pub key: StrengthKey,
    /// Where it is.
    pub state: PartState,
    /// Points earned.
    pub earned: u32,
    /// Points it is worth.
    pub weight: u32,
    /// What there is: skills listed, recent roles with results, positions…
    pub have: u32,
    /// What there should be for the full weight; 0 when it is a yes or no.
    pub of: u32,
    /// What to do about it, when it is not full: `skills`, `import`,
    /// `goals`, or `edit:<section>` (`basics`, `work`, `education`,
    /// `languages`).
    pub action: Option<String>,
}

/// How strong the profile is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StrengthView {
    /// 0 to 100.
    pub score: u8,
    /// At least 80.
    pub strong: bool,
    /// The parts, in checklist order.
    pub items: Vec<StrengthItem>,
    /// Always none: the review questions were removed (#49). Kept so the
    /// client's types stay as they are.
    pub questions_left: Option<u32>,
    /// Always none: the found skills were removed (#49). Kept so the
    /// client's types stay as they are.
    pub suggested_skills: Option<u32>,
}

/// The action for a part that is not full.
fn action_for(key: StrengthKey) -> String {
    match key {
        StrengthKey::Headline | StrengthKey::Summary | StrengthKey::Links => "edit:basics",
        StrengthKey::Achievements => "edit:work",
        StrengthKey::Skills => "skills",
        StrengthKey::Education => "edit:education",
        StrengthKey::Languages => "edit:languages",
        StrengthKey::Positions => "import",
        StrengthKey::Goals => "goals",
    }
    .to_owned()
}

/// The strength of the signed-in person's profile.
///
/// Computed from the profile and goals, never by a model. Each part that is
/// not full says what to do about it.
#[utoipa::path(
    get,
    path = "/api/v1/profile/strength",
    tag = "profile",
    operation_id = "profile.strength",
    security(("session" = [])),
    responses(
        (status = 200, description = "The strength.", body = StrengthView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get_strength(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<StrengthView>> {
    let strength = match current(&state.db, principal.account_id).await? {
        Some(current) => strength::compute(&current.profile, &current.goals),
        None => strength::compute(&Profile::default(), &Goals::default()),
    };
    Ok(Json(view_of(&strength)))
}

fn view_of(strength: &strength::Strength) -> StrengthView {
    StrengthView {
        score: strength.score,
        strong: strength.is_strong(),
        items: strength
            .items
            .iter()
            .map(|item| StrengthItem {
                key: item.key,
                state: item.state,
                earned: item.earned,
                weight: item.key.weight(),
                have: item.have,
                of: item.of,
                action: (item.state != PartState::Ok).then(|| action_for(item.key)),
            })
            .collect(),
        questions_left: None,
        suggested_skills: None,
    }
}
