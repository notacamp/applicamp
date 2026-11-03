//! `GET /api/v1/today`: where the signed-in person is, and their next step:
//! set up the profile, answer the goals, run a first search, or review new
//! matches; and the jobs that fit them found on the own job board of a
//! company they watch.

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
    features::prepare::WaitingOn,
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
    /// It lists no skills and names no technology: matches cannot cite a stack.
    pub no_skills: bool,
    /// What would make it stronger, if anything.
    pub strength_hint: Option<StrengthHint>,
}

/// Where to go to make the profile stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileAction {
    /// The Improve flow: questions about the gaps.
    Improve,
    /// Skills found in their roles.
    Skills,
    /// Import a fuller profile.
    Import,
    /// Answer the goals.
    Goals,
}

impl ProfileAction {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "improve" => Some(Self::Improve),
            "skills" => Some(Self::Skills),
            "import" => Some(Self::Import),
            "goals" => Some(Self::Goals),
            _ => None,
        }
    }
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

/// A job the app prepared as far as it could: it waits on the person.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TodayWaiting {
    pub opportunity_id: Uuid,
    pub title: String,
    pub company: String,
    /// What it waits on.
    pub waiting_on: WaitingOn,
    /// Since when.
    pub since: DateTime<Utc>,
}

/// A job that fits, found on the own job board of a company the person
/// watches, not opened yet.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompanyHit {
    /// The opportunity.
    pub id: Uuid,
    pub company: String,
    pub title: String,
    pub location: Option<String>,
    /// `remote`, `hybrid` or `onsite`, when the posting says.
    pub work_mode: Option<String>,
    /// Pay, as the posting writes it.
    pub salary_text: Option<String>,
    /// How well it fits, 0 to 100.
    pub match_score: Option<u8>,
    /// When it was found.
    pub found_at: DateTime<Utc>,
}

/// How long a job found on a watched company's board stays on Today.
const HIT_DAYS: i64 = 3;
/// Most such jobs shown.
const MOST_HITS: i64 = 5;

#[derive(sqlx::FromRow)]
struct HitRow {
    id: Uuid,
    company: String,
    title: String,
    location: Option<String>,
    work_mode: Option<String>,
    salary_text: Option<String>,
    match_score: Option<i32>,
    created_at: DateTime<Utc>,
}

/// Where the person is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Today {
    pub profile: TodayProfile,
    pub goals: TodayGoals,
    /// How strong the profile is, 0 to 100 (80 or more is strong).
    pub profile_strength: u8,
    /// Where to go to make it stronger; none once it is strong or while
    /// there is no profile.
    pub next_profile_action: Option<ProfileAction>,
    /// Questions that would make it stronger, when the AI has read it.
    pub profile_questions: Option<u32>,
    /// Found opportunities never opened (hidden ones not counted).
    pub new_matches: u32,
    /// The latest search, if any.
    pub latest_search: Option<TodaySearch>,
    /// Jobs that fit, found in the last days on the own job board of a
    /// company they watch, still Found and unopened; newest first.
    pub company_hits: Vec<CompanyHit>,
    /// Jobs prepared by the app that wait on the person, oldest first.
    pub waiting_on_you: Vec<TodayWaiting>,
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
    let now = state.clock.now();
    let hits: Vec<HitRow> = sqlx::query_as(
        "SELECT o.id, o.company, o.title, o.location, o.work_mode, o.salary_text, o.match_score,
                o.created_at
           FROM pipeline.opportunities o
          WHERE o.account_id = $1 AND o.stage = 'found' AND o.opened_at IS NULL
            AND NOT o.dismissed AND o.hidden_reason IS NULL
            AND o.created_at >= $2 AND (o.expires_at IS NULL OR o.expires_at >= $3)
            AND EXISTS (SELECT 1 FROM pipeline.opportunity_sources s
                         WHERE s.opportunity_id = o.id AND s.board = $4)
          ORDER BY o.created_at DESC, o.id
          LIMIT $5",
    )
    .bind(account)
    .bind(now - chrono::TimeDelta::days(HIT_DAYS))
    .bind(now)
    .bind(crate::store::OWN_BOARD)
    .bind(MOST_HITS)
    .fetch_all(&state.db)
    .await?;
    let waiting: Vec<(Uuid, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT o.id, o.title, o.company, p.waiting_on, p.updated_at
           FROM pipeline.preparations p
           JOIN pipeline.opportunities o ON o.id = p.opportunity_id
          WHERE p.account_id = $1 AND o.account_id = $1 AND p.state = 'waiting'
            AND p.waiting_on IS NOT NULL AND NOT o.dismissed AND o.stage <> 'found'
          ORDER BY p.updated_at, o.id
          LIMIT 20",
    )
    .bind(account)
    .fetch_all(&state.db)
    .await?;
    let waiting_on_you = waiting
        .into_iter()
        .filter_map(|(id, title, company, on, since)| {
            Some(TodayWaiting {
                opportunity_id: id,
                title,
                company,
                waiting_on: WaitingOn::parse(&on)?,
                since,
            })
        })
        .collect();
    Ok(Json(Today {
        waiting_on_you,
        next_step: next_step(&status, latest.is_some(), new_matches),
        profile: TodayProfile {
            done: status.profile_done,
            no_skills: status.no_skills,
            strength_hint: status
                .strength_hint
                .as_deref()
                .and_then(StrengthHint::parse),
        },
        goals: TodayGoals {
            answered: status.goals_answered,
            total: status.goals_total,
        },
        profile_strength: status.strength,
        next_profile_action: status.next_action.as_deref().and_then(ProfileAction::parse),
        profile_questions: status.questions_left,
        new_matches,
        latest_search: latest.map(TodaySearch::of),
        company_hits: hits
            .into_iter()
            .map(|row| CompanyHit {
                id: row.id,
                company: row.company,
                title: row.title,
                location: row.location,
                work_mode: row.work_mode,
                salary_text: row.salary_text,
                match_score: row.match_score.and_then(|n| u8::try_from(n).ok()),
                found_at: row.created_at,
            })
            .collect(),
    }))
}
