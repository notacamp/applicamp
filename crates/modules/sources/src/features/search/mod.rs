//! A search: read the approved job boards (and the company boards, when
//! they are on), merge the same job seen on several boards, credit the
//! boards of a job already in the pipeline, drop what breaks the goals'
//! hard limits on the boards' structured fields, pre-rank, score the rest
//! with the model within the spending limits (the same call reads each
//! posting's eligibility facts), drop what fails a requirement rule
//! whatever its score, and add the good matches to the pipeline as Found.
//! What it scanned on each board is kept for the Scanned view.
//!
//! - `POST /api/v1/searches` starts one ("Search now"): at most
//!   [`SourcesConfig::manual_per_day`] a day, one every
//!   [`SourcesConfig::manual_every`].
//! - `GET /api/v1/searches/latest` follows it (fetched, scored, added).
//! - `GET /api/v1/searches/estimate` says about what one would cost, and
//!   what was spent.
//! - `sources.search` runs it in the background; `sources.nightly` runs one
//!   every evening for the accounts with goals, only when the boards have
//!   jobs they have not seen.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use applicamp_ai::{Caller, Gateway, Role};
use applicamp_kernel::{
    AppState, Error, EventBus, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{Extension, Json, extract::State, http::StatusCode};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    SourcesDeps, boards, dedupe,
    events::SearchFinished,
    job::{Job, WorkMode},
    job_boards, lisbon,
    matching::{self, MatchInput, TOP},
    ports::{Attach, Scored},
    readers::{Outcome, Reader, mapping::BoardRef},
    requirements::{self, Answer, Excluded, Facts, RULES_VERSION, Salary},
    scans::{self, Scan},
    scoring::{self, BATCH, KEEP, MAX_ADDED, Read, Score},
};

/// How long a search may spend scoring before it keeps what it has.
pub const SCORING_TIME: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// A search queued or running for longer than this is taken as dead.
pub const STALE: TimeDelta = TimeDelta::minutes(30);
/// Above this estimate (in euros), the app asks before searching.
pub const CONFIRM_ABOVE_EUR: f64 = 0.05;
/// A nightly search reads the boards the nightly job read just before it:
/// a board read this long before the search started counts as read by it.
pub const NIGHTLY_READ: TimeDelta = TimeDelta::minutes(15);
/// The same company, role and place within this long is the same job.
pub const DEDUPE_WINDOW: TimeDelta = TimeDelta::days(30);

/// Where a search is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchStatus {
    Queued,
    Running,
    Finished,
    Failed,
}

/// What started a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTrigger {
    Manual,
    Nightly,
}

/// Why a search stopped early.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchStop {
    /// A spending limit was reached; what was scored before it is kept.
    AiBudgetReached,
    /// No model is configured.
    AiUnavailable,
    /// The model failed; what was scored before it is kept.
    AiFailed,
    /// No profile, or no goals answered.
    NoProfile,
    /// No board could be read.
    SourcesFailed,
}

impl SearchStop {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AiBudgetReached => "ai_budget_reached",
            Self::AiUnavailable => "ai_unavailable",
            Self::AiFailed => "ai_failed",
            Self::NoProfile => "no_profile",
            Self::SourcesFailed => "sources_failed",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        [
            Self::AiBudgetReached,
            Self::AiUnavailable,
            Self::AiFailed,
            Self::NoProfile,
            Self::SourcesFailed,
        ]
        .into_iter()
        .find(|s| s.as_str() == raw)
    }
}

/// A search.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Search {
    pub id: Uuid,
    pub status: SearchStatus,
    pub trigger: SearchTrigger,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Jobs read from the boards.
    pub fetched: u32,
    /// Jobs within the goals' hard limits and not already in the pipeline.
    pub kept: u32,
    /// Jobs scored (by the model now, or before).
    pub scored: u32,
    /// New opportunities added as Found.
    pub added: u32,
    /// Jobs scored that fail a requirement (remote only, where they live,
    /// a language, the pay floor): never added, whatever their score.
    pub dropped: u32,
    /// Why it stopped early, if it did.
    pub stopped: Option<SearchStop>,
    /// What its model calls cost, in euros.
    pub cost_eur: f64,
}

/// The latest search.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LatestSearch {
    /// None before the first.
    pub search: Option<Search>,
}

/// What was spent on AI, in euros, against the limits.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Spend {
    pub today_eur: f64,
    /// None: no daily limit.
    pub day_limit_eur: Option<f64>,
    pub month_eur: f64,
    /// None: no monthly limit.
    pub month_limit_eur: Option<f64>,
}

/// About what a search would cost now.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchEstimate {
    /// Jobs it would likely score.
    pub jobs: u32,
    /// About what that costs, in euros.
    pub cost_eur: f64,
    /// Over €0.05: ask before searching.
    pub confirm: bool,
    pub spend: Spend,
}

/// A search's row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub trigger: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub fetched: i32,
    pub kept: i32,
    pub scored: i32,
    pub added: i32,
    pub dropped: i32,
    pub batches: i32,
    pub cost_usd: f64,
    pub error_kind: Option<String>,
}

fn count(n: i32) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

impl SearchRow {
    fn view(&self, usd_to_eur: f64) -> Search {
        Search {
            id: self.id,
            status: match self.status.as_str() {
                "queued" => SearchStatus::Queued,
                "running" => SearchStatus::Running,
                "failed" => SearchStatus::Failed,
                _ => SearchStatus::Finished,
            },
            trigger: if self.trigger == "nightly" {
                SearchTrigger::Nightly
            } else {
                SearchTrigger::Manual
            },
            started_at: self.started_at,
            finished_at: self.finished_at,
            fetched: count(self.fetched),
            kept: count(self.kept),
            scored: count(self.scored),
            added: count(self.added),
            dropped: count(self.dropped),
            stopped: self.error_kind.as_deref().and_then(SearchStop::parse),
            cost_eur: self.cost_usd * usd_to_eur,
        }
    }
}

/// `account`'s latest search.
pub async fn latest_row(db: &PgPool, account: Uuid) -> Result<Option<SearchRow>> {
    Ok(sqlx::query_as(
        "SELECT id, account_id, trigger, status, started_at, finished_at, fetched, kept, scored,
                added, dropped, batches, cost_usd, error_kind
           FROM sources.searches WHERE account_id = $1
          ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(account)
    .fetch_optional(db)
    .await?)
}

fn usd_to_eur(deps: &SourcesDeps) -> f64 {
    deps.ai
        .as_ref()
        .map_or(applicamp_ai::DEFAULT_USD_TO_EUR, Gateway::usd_to_eur)
}

pub(crate) fn configured(deps: &SourcesDeps) -> Option<&Gateway> {
    deps.ai
        .as_ref()
        .filter(|ai| ai.is_configured(Role::Complete))
}

pub(crate) async fn spend(
    deps: &SourcesDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Spend> {
    let spend = match &deps.ai {
        Some(ai) => ai.spend(db, account, now).await?,
        None => applicamp_ai::Spend::default(),
    };
    Ok(Spend {
        today_eur: spend.today_eur,
        day_limit_eur: spend.day_limit_eur,
        month_eur: spend.month_eur,
        month_limit_eur: spend.month_limit_eur,
    })
}

/// Whether `spend` already reached a limit.
pub(crate) fn spent_out(spend: &Spend) -> bool {
    spend.day_limit_eur.is_some_and(|l| spend.today_eur >= l)
        || spend.month_limit_eur.is_some_and(|l| spend.month_eur >= l)
}

/// Start a search ("Search now").
///
/// Reads the job boards, keeps the jobs within the goals' hard limits,
/// scores the best 40 against the profile and goals, and adds the good
/// matches as Found; follow it with `GET /api/v1/searches/latest`. At most
/// 3 a day and one every 15 minutes (the instance may set others), never
/// past the spending limits.
#[utoipa::path(
    post,
    path = "/api/v1/searches",
    tag = "sources",
    operation_id = "sources.start",
    security(("session" = [])),
    responses(
        (status = 202, description = "Started; it runs in the background.", body = Search),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`: set up the profile and answer a goals \
            question first; `search_running`: one is already running", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`search_limit_reached`: the detail says when to try \
            again", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`: no model is set up; \
            `ai_budget_reached`: a spending limit is reached", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<(StatusCode, Json<Search>)> {
    let account = principal.account_id;
    let now = state.clock.now();
    let neighbours = deps
        .neighbours
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))?;
    if neighbours
        .matching_input(&state.db, account, now)
        .await?
        .is_none()
    {
        return Err(Error::Conflict(
            "profile_needed",
            "Set up your profile and answer at least one goals question first.".to_owned(),
        ));
    }
    if configured(&deps).is_none() {
        return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE));
    }
    let mut tx = state.db.begin().await?;
    // One start at a time per account.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('sources.search:' || $1::text))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let running: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sources.searches
                         WHERE account_id = $1 AND status IN ('queued', 'running')
                           AND started_at > $2)",
    )
    .bind(account)
    .bind(now - STALE)
    .fetch_one(&mut *tx)
    .await?;
    if running {
        return Err(Error::Conflict(
            "search_running",
            "A search is already running.".to_owned(),
        ));
    }
    let manual: Vec<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT started_at FROM sources.searches
          WHERE account_id = $1 AND trigger = 'manual' AND started_at >= $2
          ORDER BY started_at DESC",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_all(&mut *tx)
    .await?;
    let config = deps.config.as_ref();
    if let Some(most) = config.manual_per_day()
        && manual.len() as u64 >= most
    {
        return Err(Error::TooManyRequests(
            "search_limit_reached",
            format!("You can search {most} times a day. Try again tomorrow."),
        ));
    }
    if let (Some(every), Some(last)) = (config.manual_every(), manual.first())
        && now - *last < every
    {
        let minutes = ((every - (now - *last)).num_seconds() + 59) / 60;
        return Err(Error::TooManyRequests(
            "search_limit_reached",
            format!("You searched a moment ago. Try again in {minutes} minutes."),
        ));
    }
    if spent_out(&spend(&deps, &state.db, account, now).await?) {
        return Err(Error::Unavailable(applicamp_ai::AI_BUDGET_REACHED));
    }
    let row = create(&mut tx, account, "manual", now).await?;
    tx.commit().await?;
    tracing::info!(search = %row.id, "search started");
    Ok((StatusCode::ACCEPTED, Json(row.view(usd_to_eur(&deps)))))
}

/// Create a queued search for `account` and enqueue its run.
async fn create(
    conn: &mut sqlx::PgConnection,
    account: Uuid,
    trigger: &str,
    now: DateTime<Utc>,
) -> Result<SearchRow> {
    let row: SearchRow = sqlx::query_as(
        "INSERT INTO sources.searches (id, account_id, trigger, status, started_at)
         VALUES ($1, $2, $3, 'queued', $4)
         RETURNING id, account_id, trigger, status, started_at, finished_at, fetched, kept,
                   scored, added, dropped, batches, cost_usd, error_kind",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(trigger)
    .bind(now)
    .fetch_one(&mut *conn)
    .await?;
    jobs::enqueue(&mut *conn, &RunSearch { search: row.id }).await?;
    Ok(row)
}

/// The latest search.
///
/// The signed-in person's newest search, running or done: how many jobs it
/// read, kept, scored and added, why it stopped if it did, and its cost.
#[utoipa::path(
    get,
    path = "/api/v1/searches/latest",
    tag = "sources",
    operation_id = "sources.latest",
    security(("session" = [])),
    responses(
        (status = 200, description = "The latest search, or none.", body = LatestSearch),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn latest(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<LatestSearch>> {
    let row = latest_row(&state.db, principal.account_id).await?;
    Ok(Json(LatestSearch {
        search: row.map(|row| row.view(usd_to_eur(&deps))),
    }))
}

/// What a typical batch of ten costs when there is no history: about 6 000
/// tokens in and 2 500 out at the model's price.
fn typical_batch_usd(deps: &SourcesDeps) -> f64 {
    configured(deps)
        .and_then(|ai| ai.price(Role::Complete))
        .map_or(0.0, |price| price.cost(6_000, 2_500))
}

/// About what a search would cost.
///
/// The jobs it would likely score (the last search's, at most 40), times
/// what a batch of ten has cost this person on average, in euros; and what
/// they spent today and this month against their limits. Over €0.05 the
/// app asks before searching.
#[utoipa::path(
    get,
    path = "/api/v1/searches/estimate",
    tag = "sources",
    operation_id = "sources.estimate",
    security(("session" = [])),
    responses(
        (status = 200, description = "The estimate and the spending.", body = SearchEstimate),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn estimate(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<SearchEstimate>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let last = latest_row(&state.db, account).await?;
    let jobs = last
        .as_ref()
        .filter(|row| row.status == "finished")
        .map_or(TOP, |row| usize::try_from(row.kept).unwrap_or(0).min(TOP));
    let (cost, batches): (f64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(cost_usd), 0)::float8, coalesce(sum(batches), 0)::bigint
           FROM sources.searches WHERE account_id = $1 AND batches > 0",
    )
    .bind(account)
    .fetch_one(&state.db)
    .await?;
    let per_batch = if batches > 0 {
        cost / batches as f64
    } else {
        typical_batch_usd(&deps)
    };
    let cost_eur = jobs.div_ceil(BATCH) as f64 * per_batch * usd_to_eur(&deps);
    Ok(Json(SearchEstimate {
        jobs: u32::try_from(jobs).unwrap_or(0),
        cost_eur,
        confirm: cost_eur > CONFIRM_ABOVE_EUR,
        spend: spend(&deps, &state.db, account, now).await?,
    }))
}

/// Runs one search. Enqueued by "Search now" and by the nightly search.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunSearch {
    /// The search.
    pub search: Uuid,
}

impl applicamp_kernel::Job for RunSearch {
    const NAME: &'static str = "sources.search";
    const QUEUE: &'static str = "searches";
    // Never retried: a failure is recorded on the search, and a retry could
    // only spend again.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        match run(deps, &ctx.db, &ctx.events, self.search).await {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::warn!(search = %self.search, error = %e, "a search failed");
                let failed = sqlx::query(
                    "UPDATE sources.searches SET status = 'failed', finished_at = $2
                      WHERE id = $1 AND status IN ('queued', 'running')",
                )
                .bind(self.search)
                .bind(deps.clock.now())
                .execute(&ctx.db)
                .await;
                if let Err(e) = failed {
                    tracing::warn!(error = %e, "a failed search could not be marked");
                }
                Err(JobError::Permanent(e.code().to_owned()))
            }
        }
    }
}

/// The progress so far, on the search's row.
async fn progress(db: &PgPool, id: Uuid, fields: &Progress) -> Result<()> {
    sqlx::query(
        "UPDATE sources.searches
            SET fetched = $2, kept = $3, scored = $4, batches = $5, cost_usd = $6, dropped = $7
          WHERE id = $1",
    )
    .bind(id)
    .bind(i32::try_from(fields.fetched).unwrap_or(i32::MAX))
    .bind(i32::try_from(fields.kept).unwrap_or(i32::MAX))
    .bind(i32::try_from(fields.scored).unwrap_or(i32::MAX))
    .bind(i32::try_from(fields.batches).unwrap_or(i32::MAX))
    .bind(fields.cost_usd)
    .bind(i32::try_from(fields.dropped).unwrap_or(i32::MAX))
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Default)]
struct Progress {
    fetched: usize,
    kept: usize,
    scored: usize,
    dropped: usize,
    batches: usize,
    cost_usd: f64,
}

/// End the search: its counts, why it stopped, and `sources.search_finished`.
async fn finish(
    db: &PgPool,
    events: &EventBus,
    row: &SearchRow,
    fields: &Progress,
    added: u32,
    stopped: Option<SearchStop>,
    now: DateTime<Utc>,
) -> Result<()> {
    progress(db, row.id, fields).await?;
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE sources.searches
            SET status = 'finished', finished_at = $2, added = $3, error_kind = $4
          WHERE id = $1",
    )
    .bind(row.id)
    .bind(now)
    .bind(i32::try_from(added).unwrap_or(i32::MAX))
    .bind(stopped.map(SearchStop::as_str))
    .execute(&mut *tx)
    .await?;
    events
        .publish(
            &mut tx,
            &SearchFinished {
                account: row.account_id,
                search: row.id,
                added,
            },
        )
        .await?;
    tx.commit().await?;
    // Ids and counts only.
    tracing::info!(
        search = %row.id,
        fetched = fields.fetched,
        kept = fields.kept,
        scored = fields.scored,
        dropped = fields.dropped,
        batches = fields.batches,
        added,
        cost_usd = fields.cost_usd,
        stopped = stopped.map_or("no", SearchStop::as_str),
        "search finished"
    );
    Ok(())
}

/// A cached score's row.
#[derive(sqlx::FromRow)]
struct CachedRow {
    source: String,
    source_ref: String,
    match_score: i32,
    why: sqlx::types::Json<Vec<String>>,
    gaps: sqlx::types::Json<Vec<String>>,
    read: Option<sqlx::types::Json<Read>>,
    remote: Option<String>,
    open_to_candidate: Option<String>,
    region_note: Option<String>,
    working_languages: Option<Vec<String>>,
    salary: Option<sqlx::types::Json<Salary>>,
}

/// Scores already given to `account` for these versions of the profile,
/// the goals and the rules, by job key.
async fn cached_scores(
    db: &PgPool,
    account: Uuid,
    input: &MatchInput,
) -> Result<HashMap<(String, String), Score>> {
    let rows: Vec<CachedRow> = sqlx::query_as(
        "SELECT source, source_ref, match_score, why, gaps, read, remote, open_to_candidate,
                region_note, working_languages, salary
           FROM sources.scores
          WHERE account_id = $1 AND profile_version = $2 AND goals_version = $3
            AND rules_version = $4",
    )
    .bind(account)
    .bind(&input.profile_version)
    .bind(input.goals_version)
    .bind(RULES_VERSION)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let answer = |raw: Option<&str>| raw.map_or(Answer::Unclear, Answer::parse);
            (
                (row.source, row.source_ref),
                Score {
                    match_score: u8::try_from(row.match_score.clamp(0, 100)).unwrap_or(0),
                    why: row.why.0,
                    gaps: row.gaps.0,
                    read: row.read.map(|r| r.0),
                    facts: Facts {
                        remote: answer(row.remote.as_deref()),
                        open_to_candidate: answer(row.open_to_candidate.as_deref()),
                        region_note: row.region_note,
                        working_languages: row.working_languages.unwrap_or_default(),
                        salary: row.salary.map(|s| s.0),
                    },
                },
            )
        })
        .collect())
}

/// Keep `score` (with its facts, and the reason `job` fails a requirement,
/// if it does) for these versions.
async fn save_score(
    db: &PgPool,
    account: Uuid,
    input: &MatchInput,
    job: &Job,
    score: &Score,
    excluded: Option<Excluded>,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources.scores
            (account_id, source, source_ref, profile_version, goals_version, rules_version,
             match_score, why, gaps, read, remote, open_to_candidate, region_note,
             working_languages, salary, excluded_reason, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
         ON CONFLICT DO NOTHING",
    )
    .bind(account)
    .bind(job.source_name())
    .bind(&job.source_ref)
    .bind(&input.profile_version)
    .bind(input.goals_version)
    .bind(RULES_VERSION)
    .bind(i32::from(score.match_score))
    .bind(sqlx::types::Json(&score.why))
    .bind(sqlx::types::Json(&score.gaps))
    .bind(score.read.as_ref().map(sqlx::types::Json))
    .bind(score.facts.remote.as_str())
    .bind(score.facts.open_to_candidate.as_str())
    .bind(&score.facts.region_note)
    .bind(&score.facts.working_languages)
    .bind(score.facts.salary.as_ref().map(sqlx::types::Json))
    .bind(excluded.map(Excluded::as_str))
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// Where `job` is: as the board says, or as the model read a free-text post.
pub(crate) fn place_of<'a>(job: &'a Job, score: &'a Score) -> Option<&'a str> {
    if job.needs_reading() {
        score
            .read
            .as_ref()
            .and_then(|read| read.location.as_deref())
    } else {
        job.location.as_deref()
    }
}

/// `job` with what the model read from a free-text post; `None` when it is
/// one the model could not read (no company or role).
fn with_reading(mut job: Job, score: &Score) -> Option<Job> {
    if !job.needs_reading() {
        return Some(job);
    }
    let read = score.read.as_ref()?;
    job.company = read.company.clone()?;
    job.title = read.title.clone()?;
    job.location.clone_from(&read.location);
    job.work_mode = read.work_mode.as_deref().and_then(WorkMode::parse);
    job.salary_text.clone_from(&read.salary_text);
    Some(job)
}

/// The jobs `boards` have, once each.
fn unique(jobs: Vec<Job>) -> Vec<Job> {
    let mut seen = HashSet::new();
    jobs.into_iter()
        .filter(|job| seen.insert(job.key()))
        .collect()
}

/// How reading one job board went, for the Scanned view.
#[derive(Debug, Clone)]
pub(crate) struct Scanned {
    pub board: BoardRef,
    pub outcome: Outcome,
    pub error: Option<String>,
    pub duration_ms: u32,
    /// Jobs it listed.
    pub read: u32,
}

/// What the sources gave.
#[derive(Debug, Default)]
pub(crate) struct Reading {
    /// Every job once, the same job on several boards merged.
    pub jobs: Vec<Job>,
    /// Each job board read.
    pub boards: Vec<Scanned>,
    /// Boards (and pages) that could not be read.
    pub failed: u32,
}

/// Read the approved job boards, and the company boards when they are on.
pub(crate) async fn read_sources(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    now: DateTime<Utc>,
    fresh_since: DateTime<Utc>,
) -> Result<Reading> {
    let rows = job_boards::approved(db).await?;
    let mut reading = Reading::default();
    let mut jobs = Vec::new();
    for read in reader.job_boards(db, &rows, now, fresh_since).await {
        if read.outcome == Outcome::Error {
            reading.failed += 1;
        }
        reading.boards.push(Scanned {
            read: u32::try_from(read.jobs.len()).unwrap_or(u32::MAX),
            board: read.board,
            outcome: read.outcome,
            error: read.error,
            duration_ms: read.duration_ms,
        });
        jobs.extend(read.jobs);
    }
    if deps.config.company_boards() {
        let company = boards::all(db).await?;
        let fetched = reader.all(db, &company).await;
        reading.failed += fetched.failed;
        jobs.extend(fetched.jobs);
    }
    reading.jobs = dedupe::merge(unique(jobs));
    Ok(reading)
}

/// What one search did on one board.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    passed: u32,
    scored: u32,
    matched: u32,
    cost_usd: f64,
}

/// The job boards `job` was seen on.
fn boards_of(job: &Job) -> Vec<String> {
    job.board
        .as_ref()
        .map(|b| {
            let mut slugs: Vec<String> = b.links.iter().map(|l| l.board.clone()).collect();
            if slugs.is_empty() {
                slugs.push(b.slug.clone());
            }
            slugs
        })
        .unwrap_or_default()
}

fn tally<'a>(
    tallies: &'a mut HashMap<String, Tally>,
    job: &Job,
) -> impl Iterator<Item = &'a mut Tally> {
    let slugs = boards_of(job);
    tallies
        .iter_mut()
        .filter(move |(slug, _)| slugs.contains(slug))
        .map(|(_, t)| t)
}

/// Keep what search `row` scanned on each job board.
async fn record_scans(
    deps: &SourcesDeps,
    db: &PgPool,
    row: &SearchRow,
    boards: &[Scanned],
    tallies: &HashMap<String, Tally>,
    now: DateTime<Utc>,
) -> Result<()> {
    let rate = usd_to_eur(deps);
    let rows: Vec<Scan> = boards
        .iter()
        .map(|scanned| {
            let t = tallies
                .get(&scanned.board.slug)
                .copied()
                .unwrap_or_default();
            Scan {
                board: scanned.board.id,
                outcome: scanned.outcome.as_str(),
                jobs_read: scanned.read,
                passed_requirements: t.passed,
                scored_by_ai: t.scored,
                matched: t.matched,
                ai_cost_eur: t.cost_usd * rate,
                error_kind: scanned.error.clone(),
                duration_ms: scanned.duration_ms,
                note: None,
            }
        })
        .collect();
    scans::record(
        db,
        scans::Run::Search {
            account: row.account_id,
            search: row.id,
        },
        &rows,
        now,
    )
    .await?;
    scans::prune(db, now).await?;
    Ok(())
}

/// Run search `id` (idempotent: a finished one is left alone).
pub(crate) async fn run(
    deps: &SourcesDeps,
    db: &PgPool,
    events: &EventBus,
    id: Uuid,
) -> Result<()> {
    let row: Option<SearchRow> = sqlx::query_as(
        "SELECT id, account_id, trigger, status, started_at, finished_at, fetched, kept, scored,
                added, dropped, batches, cost_usd, error_kind
           FROM sources.searches WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let Some(row) = row.filter(|r| r.status == "queued" || r.status == "running") else {
        return Ok(());
    };
    sqlx::query("UPDATE sources.searches SET status = 'running' WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    let account = row.account_id;
    let mut done = Progress::default();
    let (neighbours, reader) = match (&deps.neighbours, &deps.reader) {
        (Some(n), Some(r)) => (n, r),
        _ => return Err(Error::internal("the sources module is not composed")),
    };
    let now = deps.clock.now();
    let Some(input) = neighbours.matching_input(db, account, now).await? else {
        return finish(db, events, &row, &done, 0, Some(SearchStop::NoProfile), now).await;
    };
    let Some(ai) = configured(deps) else {
        return finish(
            db,
            events,
            &row,
            &done,
            0,
            Some(SearchStop::AiUnavailable),
            now,
        )
        .await;
    };

    // Read the boards.
    let fresh_since = if row.trigger == "nightly" {
        row.started_at - NIGHTLY_READ
    } else {
        row.started_at
    };
    let reading = read_sources(deps, reader, db, now, fresh_since).await?;
    let failed = reading.failed;
    let scanned = reading.boards;
    let mut jobs = reading.jobs;
    let mut tallies: HashMap<String, Tally> = scanned
        .iter()
        .map(|s| (s.board.slug.clone(), Tally::default()))
        .collect();
    done.fetched = jobs.len();
    progress(db, id, &done).await?;
    if jobs.is_empty() && failed > 0 {
        let now = deps.clock.now();
        record_scans(deps, db, &row, &scanned, &tallies, now).await?;
        return finish(
            db,
            events,
            &row,
            &done,
            0,
            Some(SearchStop::SourcesFailed),
            now,
        )
        .await;
    }

    // A job already in the pipeline from another board: credit this board
    // on it, and read it no further.
    let known = neighbours.known_jobs(db, account).await?;
    let links = neighbours
        .known_links(db, account, now - DEDUPE_WINDOW)
        .await?;
    let mut attach: Vec<Attach> = Vec::new();
    jobs.retain(|job| {
        let same = known.contains(&job.key());
        let url = dedupe::job_url(job);
        let key = dedupe::dedupe_key(job);
        let elsewhere = !same
            && (url.as_ref().is_some_and(|u| links.urls.contains(u))
                || key.as_ref().is_some_and(|k| links.keys.contains(k)));
        if let Some(board) = &job.board
            && (elsewhere || (same && board.links.len() > 1))
        {
            attach.push(Attach {
                canonical_url: url,
                dedupe_key: key,
                links: board.links.clone(),
            });
        }
        !(same || elsewhere)
    });
    if !attach.is_empty() {
        neighbours
            .attach_sources(db, account, attach, deps.clock.now())
            .await?;
    }

    // The hard limits, then the best 40.
    let candidates: Vec<Job> = jobs
        .into_iter()
        .filter(|job| matching::hard_limit(job, &input, &known, now).is_none())
        .collect();
    for job in &candidates {
        for t in tally(&mut tallies, job) {
            t.passed += 1;
        }
    }
    done.kept = candidates.len();
    let mut top = matching::pre_rank(candidates, &input, now);
    let cached = cached_scores(db, account, &input).await?;
    for job in top
        .iter_mut()
        .filter(|job| !cached.contains_key(&job.key()))
    {
        if let Err(e) = reader.describe(db, job).await {
            tracing::info!(error = %e, "a job's description could not be read");
        }
    }
    let mut scored: Vec<(Job, Score)> = Vec::new();
    let mut unscored: Vec<Job> = Vec::new();
    for job in top {
        match cached.get(&job.key()) {
            Some(score) => scored.push((job, score.clone())),
            None => unscored.push(job),
        }
    }
    done.scored = scored.len();
    progress(db, id, &done).await?;

    // Score the rest, ten at a time, within the limits.
    let mut stopped = None;
    let deadline = std::time::Instant::now() + SCORING_TIME;
    for batch in unscored.chunks(BATCH) {
        if std::time::Instant::now() > deadline {
            break;
        }
        let jobs: Vec<&Job> = batch.iter().collect();
        let caller = Caller {
            db,
            account,
            now: deps.clock.now(),
        };
        match scoring::score(ai, caller, &input, &jobs).await {
            Ok((scores, cost)) => {
                done.batches += 1;
                done.cost_usd += cost;
                // Each job of the batch bears its share, split between the
                // boards it was seen on. Ten at most: exact as f64.
                let share = cost / batch.len() as f64;
                for job in batch {
                    let boards = boards_of(job).len().max(1) as f64;
                    for t in tally(&mut tallies, job) {
                        t.cost_usd += share / boards;
                    }
                }
                for (index, score) in scores {
                    let job = &batch[index];
                    let excluded =
                        requirements::check(&input, &score.facts, place_of(job, &score)).excluded;
                    save_score(db, account, &input, job, &score, excluded, deps.clock.now())
                        .await?;
                    scored.push((job.clone(), score));
                }
                done.scored = scored.len();
                progress(db, id, &done).await?;
            }
            Err(e) => {
                let stop = match e {
                    applicamp_ai::Error::OverBudget { .. }
                    | applicamp_ai::Error::OverDailyCap { .. } => SearchStop::AiBudgetReached,
                    applicamp_ai::Error::NotConfigured(_) => SearchStop::AiUnavailable,
                    _ => {
                        // A call was made: count it.
                        done.batches += 1;
                        tracing::warn!(error = %e, "a scoring call failed; the search keeps what it has");
                        SearchStop::AiFailed
                    }
                };
                stopped = Some(stop);
                break;
            }
        }
    }

    for (job, _) in &scored {
        for t in tally(&mut tallies, job) {
            t.scored += 1;
        }
    }

    // Keep the good ones that meet every requirement: the rules decide on
    // the facts, never the score.
    let mut keep: Vec<Scored> = Vec::new();
    for (job, mut score) in scored {
        let verdict = requirements::check(&input, &score.facts, place_of(&job, &score));
        if verdict.excluded.is_some() {
            done.dropped += 1;
            continue;
        }
        if score.match_score < KEEP {
            continue;
        }
        let Some(job) = with_reading(job, &score) else {
            continue;
        };
        score.gaps.extend(verdict.gaps);
        keep.push(Scored { job, score });
    }
    keep.sort_by_key(|scored| std::cmp::Reverse(scored.score.match_score));
    keep.truncate(MAX_ADDED);
    for scored in &keep {
        for t in tally(&mut tallies, &scored.job) {
            t.matched += 1;
        }
    }
    let now = deps.clock.now();
    record_scans(deps, db, &row, &scanned, &tallies, now).await?;
    let added = neighbours.add_found(db, events, account, keep, now).await?;
    finish(db, events, &row, &done, added, stopped, now).await
}

/// The nightly search: every evening at 21:00 Lisbon time, for each account
/// with goals, only when the boards have jobs within their limits that were
/// never scored for them, and never for an account already at a spending
/// limit. Schedules the next one first.
#[derive(Debug, Serialize, Deserialize)]
pub struct Nightly {}

impl applicamp_kernel::Job for Nightly {
    const NAME: &'static str = "sources.nightly";
    const QUEUE: &'static str = "searches";
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_nightly(&ctx.db, deps).await?;
        nightly(deps, &ctx.db).await.map_err(JobError::from)
    }
}

/// Make sure the next nightly search is scheduled, when nightly searches
/// are on. Whether one was added.
pub async fn schedule_nightly(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.nightly() {
        return Ok(false);
    }
    let at = lisbon::next_run(deps.clock.now());
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &Nightly {}, at).await?;
    tx.commit().await?;
    Ok(added)
}

async fn nightly(deps: &SourcesDeps, db: &PgPool) -> Result<()> {
    let (Some(neighbours), Some(reader)) = (&deps.neighbours, &deps.reader) else {
        return Ok(());
    };
    if configured(deps).is_none() {
        return Ok(());
    }
    let accounts = neighbours.accounts_with_goals(db).await?;
    if accounts.is_empty() {
        return Ok(());
    }
    // Read once: the searches it starts read the same answers from the
    // cache (within each board's polling interval).
    let now = deps.clock.now();
    let jobs = read_sources(deps, reader, db, now, now).await?.jobs;
    let mut started = 0_u32;
    for account in accounts {
        let now = deps.clock.now();
        let Some(input) = neighbours.matching_input(db, account, now).await? else {
            continue;
        };
        if spent_out(&spend(deps, db, account, now).await?) {
            continue;
        }
        let known = neighbours.known_jobs(db, account).await?;
        let cached = cached_scores(db, account, &input).await?;
        let unseen = jobs.iter().any(|job| {
            matching::hard_limit(job, &input, &known, now).is_none()
                && !cached.contains_key(&job.key())
        });
        if !unseen {
            continue;
        }
        let mut tx = db.begin().await?;
        let busy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sources.searches
                             WHERE account_id = $1 AND status IN ('queued', 'running')
                               AND started_at > $2)",
        )
        .bind(account)
        .bind(now - STALE)
        .fetch_one(&mut *tx)
        .await?;
        if !busy {
            create(&mut tx, account, "nightly", now).await?;
            started += 1;
        }
        tx.commit().await?;
    }
    tracing::info!(started, "nightly searches started");
    Ok(())
}

/// Read the posting at `link` when it is a Greenhouse, Lever or Ashby one,
/// and add its board to the boards searched (for `account`'s later
/// searches and everyone's). `None` for any other link, or one that could
/// not be read: the caller keeps it as a plain link.
pub async fn read_posting(
    deps: &SourcesDeps,
    db: &PgPool,
    account: Uuid,
    link: &url::Url,
) -> Result<Option<Job>> {
    let Some(reader) = &deps.reader else {
        return Ok(None);
    };
    let Some((kind, token, _, _)) = crate::readers::posting_link(link) else {
        return Ok(None);
    };
    let known_company = boards::company(db, kind, &token).await?;
    match reader.posting(db, link, |_, _| known_company.clone()).await {
        Ok(Some((mut job, board))) => {
            if let Err(e) = reader.describe(db, &mut job).await {
                tracing::info!(error = %e, "a pasted posting's description could not be read");
            }
            boards::add(db, &board, account).await?;
            Ok(Some(job))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::info!(source = kind.as_str(), error = %e, "a pasted posting could not be read");
            Ok(None)
        }
    }
}
