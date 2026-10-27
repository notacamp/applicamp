//! `GET /api/v1/today`: where the signed-in person is, and their next step:
//! set up the profile, answer the goals, run a first search, or review new
//! matches.

#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{Extension, Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    PipelineDeps,
    ports::{ProfileStatus, SearchLine},
};

/// What would make the profile stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrengthHint {
    AddHeadline,
    AddWork,
    AddSkills,
}

impl StrengthHint {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "add_headline" => Some(Self::AddHeadline),
            "add_work" => Some(Self::AddWork),
            "add_skills" => Some(Self::AddSkills),
            _ => None,
        }
    }
}

/// The profile, on Today.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TodayProfile {
    /// There is a profile with something in it.
    pub done: bool,
    /// What would make it stronger, if anything.
    pub strength_hint: Option<StrengthHint>,
}

/// The goals, on Today.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TodayGoals {
    /// Questions answered.
    pub answered: u32,
    /// Questions there are.
    pub total: u32,
}

/// Where a search is. The same as the search's own (`sources`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchStatus {
    Queued,
    Running,
    Finished,
    Failed,
}

/// What started a search. The same as the search's own (`sources`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTrigger {
    Manual,
    Nightly,
}

/// Why a search stopped early. The same as the search's own (`sources`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchStop {
    AiBudgetReached,
    AiUnavailable,
    AiFailed,
    NoProfile,
    SourcesFailed,
}

impl SearchStop {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ai_budget_reached" => Some(Self::AiBudgetReached),
            "ai_unavailable" => Some(Self::AiUnavailable),
            "ai_failed" => Some(Self::AiFailed),
            "no_profile" => Some(Self::NoProfile),
            "sources_failed" => Some(Self::SourcesFailed),
            _ => None,
        }
    }
}

/// The latest search, on Today.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TodaySearch {
    pub id: Uuid,
    pub status: SearchStatus,
    pub trigger: SearchTrigger,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Jobs read.
    pub fetched: u32,
    /// New opportunities it added.
    pub added: u32,
    /// Why it stopped early, if it did.
    pub stopped: Option<SearchStop>,
}

impl TodaySearch {
    fn of(line: SearchLine) -> Self {
        Self {
            id: line.id,
            status: match line.status.as_str() {
                "queued" => SearchStatus::Queued,
                "running" => SearchStatus::Running,
                "failed" => SearchStatus::Failed,
                _ => SearchStatus::Finished,
            },
            trigger: if line.trigger == "nightly" {
                SearchTrigger::Nightly
            } else {
                SearchTrigger::Manual
            },
            started_at: line.started_at,
            finished_at: line.finished_at,
            fetched: u32::try_from(line.fetched).unwrap_or(0),
            added: u32::try_from(line.added).unwrap_or(0),
            stopped: line.stopped.as_deref().and_then(SearchStop::parse),
        }
    }
}

/// What to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    /// Import or write a profile.
    SetUpProfile,
    /// Answer the goals questions.
    AnswerGoals,
    /// Run a first search.
    Search,
    /// Look at the new matches.
    ReviewMatches,
    /// Nothing needs them.
    None,
}

/// Where the person is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Today {
    pub profile: TodayProfile,
    pub goals: TodayGoals,
    /// Found opportunities never opened (hidden ones not counted).
    pub new_matches: u32,
    /// The latest search, if any.
    pub latest_search: Option<TodaySearch>,
    pub next_step: NextStep,
}

/// The next step for a person in this state.
pub(crate) fn next_step(status: &ProfileStatus, searched: bool, new_matches: u32) -> NextStep {
    if !status.profile_done {
        NextStep::SetUpProfile
    } else if status.goals_answered == 0 {
        NextStep::AnswerGoals
    } else if !searched {
        NextStep::Search
    } else if new_matches > 0 {
        NextStep::ReviewMatches
    } else {
        NextStep::None
    }
}

/// Today.
///
/// How far the signed-in person is with their profile and goals, how many
/// new matches wait for them, their latest search, and the next step.
#[utoipa::path(
    get,
    path = "/api/v1/today",
    tag = "pipeline",
    operation_id = "pipeline.today",
    security(("session" = [])),
    responses(
        (status = 200, description = "Where they are.", body = Today),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
) -> Result<Json<Today>> {
    let account = principal.account_id;
    let neighbours = deps
        .neighbours
        .as_ref()
        .ok_or_else(|| Error::internal("the pipeline module is not composed"))?;
    let status = neighbours.profile_status(&state.db, account).await?;
    let latest = neighbours.latest_search(&state.db, account).await?;
    let new_matches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.opportunities
          WHERE account_id = $1 AND stage = 'found' AND opened_at IS NULL AND NOT dismissed
            AND hidden_reason IS NULL AND (expires_at IS NULL OR expires_at >= $2)",
    )
    .bind(account)
    .bind(state.clock.now())
    .fetch_one(&state.db)
    .await?;
    let new_matches = u32::try_from(new_matches).unwrap_or(u32::MAX);
    Ok(Json(Today {
        next_step: next_step(&status, latest.is_some(), new_matches),
        profile: TodayProfile {
            done: status.profile_done,
            strength_hint: status
                .strength_hint
                .as_deref()
                .and_then(StrengthHint::parse),
        },
        goals: TodayGoals {
            answered: status.goals_answered,
            total: status.goals_total,
        },
        new_matches,
        latest_search: latest.map(TodaySearch::of),
    }))
}
