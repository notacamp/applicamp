//! A search: read the approved job boards (asked for the person's roles and
//! top skills when they can search) and the company boards, when they are
//! on; merge the same job seen on several boards, credit the boards of a job
//! already in the pipeline, drop what breaks the goals' hard limits on the
//! boards' structured fields; give every job left without a full read a
//! quick look ([`crate::quick`], 25 a call) when there are more than one
//! search reads in full, and read the best in full
//! ([`crate::SourcesConfig::full_scores_per_search`], 100) with the model
//! within the spending limits (the same call reads each posting's
//! eligibility facts); drop what fails a requirement rule whatever its
//! score, and add the good matches to the pipeline as Found. What it did
//! with every job read (the funnel, [`JobsLeftOut`]) and on each board is kept
//! for the Searches tab.
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
    features::{store, watch},
    job::{Job, WorkMode},
    job_boards::{self, BoardRow},
    lisbon,
    matching::{self, MatchInput, Out},
    ports::{Attach, OwnBoard, Scored},
    quick::{self, Look},
    readers::{Outcome, PartialRead, Reader, mapping::BoardRef},
    requirements::{self, Answer, Excluded, Facts, RULES_VERSION, Salary},
    scans::{self, Scan},
    scoring::{self, BATCH, KEEP, MAX_ADDED, Read, Score},
};

/// How long a search may spend on the model (quick looks and full reads
/// together) before it keeps what it has.
pub const SCORING_TIME: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// The quick looks stop after this much of [`SCORING_TIME`], so the full
/// reads always get their turn.
pub const QUICK_LOOK_TIME: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// Failed model calls in a row that end a search's scoring (the provider
/// is down, not one bad answer).
pub const FAILED_IN_A_ROW: u32 = 4;
/// A search queued or running for longer than this is taken as dead.
pub const STALE: TimeDelta = TimeDelta::minutes(30);
/// Above this estimate (in euros), the app asks before searching; below it
/// the cost is only said on the line under Search now.
pub const CONFIRM_ABOVE_EUR: f64 = 0.25;
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

/// Why the jobs a search read were left out, by reason. With the good
/// matches they add up to the jobs read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobsLeftOut {
    /// Already in the person's jobs, judged by an earlier search, or a match
    /// the pipeline already had.
    #[serde(default)]
    pub seen_before: u32,
    /// Older than 30 days, or closed.
    #[serde(default)]
    pub too_old: u32,
    /// Not remote, or not where they can work.
    #[serde(default)]
    pub not_remote: u32,
    /// Not among the best at the quick look.
    #[serde(default)]
    pub quick_look: u32,
    /// Not among the closest to their roles and skills today, so not given
    /// a quick look (found without a model, from the words of the title,
    /// the company and the site's tags, and how new it is).
    #[serde(default)]
    pub preselect: u32,
    /// In a language they don't work in.
    #[serde(default)]
    pub language: u32,
    /// Hiring only in other places.
    #[serde(default)]
    pub not_open_to_location: u32,
    /// Paying less than their floor.
    #[serde(default)]
    pub below_floor: u32,
    /// Read in full, not a good match.
    #[serde(default)]
    pub not_good_enough: u32,
    /// Left for the next search: a spending limit, the time, a failed call,
    /// or the most one search adds.
    #[serde(default)]
    pub waiting: u32,
}

impl JobsLeftOut {
    /// Every job left out.
    pub fn total(&self) -> u32 {
        [
            self.seen_before,
            self.too_old,
            self.not_remote,
            self.quick_look,
            self.preselect,
            self.language,
            self.not_open_to_location,
            self.below_floor,
            self.not_good_enough,
            self.waiting,
        ]
        .into_iter()
        .fold(0_u32, u32::saturating_add)
    }

    /// A job left out by a hard limit.
    fn hard_limit(&mut self, out: Out) {
        let count = match out {
            Out::Known => &mut self.seen_before,
            Out::TooOld | Out::Expired => &mut self.too_old,
            Out::WorkMode | Out::Place | Out::NotRemote => &mut self.not_remote,
            Out::Region => &mut self.not_open_to_location,
        };
        add(count, 1);
    }

    /// A job read in full now that fails a requirement.
    fn requirement(&mut self, reason: Excluded) {
        let count = match reason {
            Excluded::NotRemote => &mut self.not_remote,
            Excluded::NotOpenToLocation => &mut self.not_open_to_location,
            Excluded::Language => &mut self.language,
            Excluded::BelowFloor => &mut self.below_floor,
        };
        add(count, 1);
    }
}

/// `n` more in `count`.
fn add(count: &mut u32, n: usize) {
    *count = count.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
}

/// What a search did with the jobs it read, for the Searches tab.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchFunnel {
    /// Jobs read from the sites.
    pub read: u32,
    /// Job sites the job store gave this search jobs from; none for a search
    /// from before searches read the store.
    pub from_sites: Option<u32>,
    /// Company pages the job store gave this search jobs from.
    pub from_company_pages: Option<u32>,
    /// Jobs given a quick look.
    pub quick_looked: u32,
    /// Jobs read in full.
    pub read_in_full: u32,
    /// Good matches added to the person's jobs.
    pub good_matches: u32,
    /// Of the jobs read in full now, the good matches kept although their
    /// posting does not say whether it hires where the person lives: they
    /// carry a note saying so.
    pub kept_with_note: u32,
    /// Why the others were left out; none for a search from before these
    /// were counted.
    pub left_out: Option<JobsLeftOut>,
    /// Why it stopped early, if it did.
    pub stopped: Option<SearchStop>,
}

/// A search's funnel, from its row's counts.
pub(crate) fn funnel_of(row: &scans::RunRow) -> SearchFunnel {
    let n = |v: Option<i32>| v.and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    SearchFunnel {
        read: n(row.fetched),
        from_sites: row.from_sites.and_then(|v| u32::try_from(v).ok()),
        from_company_pages: row.from_company_pages.and_then(|v| u32::try_from(v).ok()),
        quick_looked: n(row.quick_looked),
        // A search from before the funnel: what it scored.
        read_in_full: n(row.read_in_full.or(row.scored)),
        good_matches: n(row.added),
        kept_with_note: n(row.kept_with_gap),
        left_out: row
            .dropped_by_reason
            .as_ref()
            .and_then(|raw| serde_json::from_value(raw.0.clone()).ok()),
        stopped: row.error_kind.as_deref().and_then(SearchStop::parse),
    }
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

/// What a typical call costs when there is no history: about 6 000 tokens
/// in and 2 500 out (ten full reads) at the model's price; a quick look at
/// 25 costs about the same or less.
fn typical_batch_usd(deps: &SourcesDeps) -> f64 {
    configured(deps)
        .and_then(|ai| ai.price(Role::Complete))
        .map_or(0.0, |price| price.cost(6_000, 2_500))
}

/// The model calls a search makes for `kept` jobs within the limits, reading
/// at most `slots` in full, `quick_looked` of them looked at quickly first
/// (`None`: all of them when they are more than `slots`).
fn calls_for(kept: usize, slots: usize, quick_looked: Option<usize>) -> usize {
    let full = kept.min(slots).div_ceil(BATCH);
    let looks = quick_looked.unwrap_or(if kept > slots { kept } else { 0 });
    full + looks.div_ceil(quick::BATCH)
}

/// About what a search would cost.
///
/// The jobs it would likely look at (the last search's within the limits),
/// the model calls that takes (a quick look for 25, a full read for 10, at
/// most 100 read in full), times what a call has cost this person on
/// average, in euros; and what they spent today and this month against
/// their limits. Over €0.25 the app asks before searching.
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
    let slots = deps.config.full_scores_per_search();
    let last = latest_row(&state.db, account)
        .await?
        .filter(|row| row.status == "finished");
    let previous = last
        .as_ref()
        .map_or(slots, |row| usize::try_from(row.kept).unwrap_or(0));
    let quick_looked: Option<i32> = match &last {
        Some(row) => {
            sqlx::query_scalar("SELECT quick_looked FROM sources.searches WHERE id = $1")
                .bind(row.id)
                .fetch_one(&state.db)
                .await?
        }
        None => None,
    };
    // The jobs of the store that have no score for this person yet: what a
    // search would give a quick look, the closest first, at most
    // `QUICK_LOOK_PER_SEARCH`.
    let mut looks = 0_usize;
    if deps.config.store()
        && let Some(neighbours) = &deps.neighbours
        && let Some(input) = neighbours.matching_input(&state.db, account, now).await?
    {
        let sites: Vec<Uuid> = on_sites(&deps, &state.db, account)
            .await?
            .iter()
            .map(|row| row.id)
            .collect();
        let unscored = store::unscored(
            &state.db,
            account,
            &sites,
            (&input.profile_version, input.goals_version, RULES_VERSION),
            now,
        )
        .await?;
        looks = usize::try_from(unscored)
            .unwrap_or(usize::MAX)
            .min(deps.config.quick_look_per_search());
    }
    let (jobs, calls) = if looks > 0 {
        (
            looks,
            calls_for(looks.max(previous.min(slots)), slots, Some(looks)),
        )
    } else {
        (
            previous,
            calls_for(
                previous,
                slots,
                quick_looked.and_then(|n| usize::try_from(n).ok()),
            ),
        )
    };
    let per_batch = per_batch_usd(&deps, &state.db, account).await?;
    // A few dozen calls: exact as f64.
    let cost_eur = calls as f64 * per_batch * usd_to_eur(&deps);
    Ok(Json(SearchEstimate {
        jobs: u32::try_from(jobs).unwrap_or(u32::MAX),
        cost_eur,
        confirm: cost_eur > CONFIRM_ABOVE_EUR,
        spend: spend(&deps, &state.db, account, now).await?,
    }))
}

/// What one model call has cost `account` on average, in dollars (a typical
/// call's price before there is any history).
async fn per_batch_usd(deps: &SourcesDeps, db: &PgPool, account: Uuid) -> Result<f64> {
    let (cost, batches): (f64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(cost_usd), 0)::float8, coalesce(sum(batches), 0)::bigint
           FROM sources.searches WHERE account_id = $1 AND batches > 0",
    )
    .bind(account)
    .fetch_one(db)
    .await?;
    Ok(if batches > 0 {
        cost / batches as f64
    } else {
        typical_batch_usd(deps)
    })
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
    let int = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    sqlx::query(
        "UPDATE sources.searches
            SET fetched = $2, kept = $3, scored = $4, batches = $5, cost_usd = $6, dropped = $7,
                not_grounded = $8, quick_looked = $9, read_in_full = $10, from_sites = $11,
                from_company_pages = $12, expected_cost_usd = $13
          WHERE id = $1",
    )
    .bind(id)
    .bind(int(fields.fetched))
    .bind(int(fields.kept))
    .bind(int(fields.scored))
    .bind(int(fields.batches))
    .bind(fields.cost_usd)
    .bind(int(fields.dropped))
    .bind(int(fields.not_grounded))
    .bind(int(fields.quick_looked))
    .bind(int(fields.read_in_full))
    .bind(int(fields.from_sites))
    .bind(int(fields.from_company_pages))
    .bind(fields.expected_cost_usd)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Default)]
struct Progress {
    fetched: usize,
    kept: usize,
    /// Jobs with a full read, now or by an earlier search.
    scored: usize,
    /// Jobs read in full that fail a requirement (now or before).
    dropped: usize,
    /// Jobs whose every reason claimed something the profile does not say.
    not_grounded: usize,
    batches: usize,
    cost_usd: f64,
    /// Jobs given a quick look now.
    quick_looked: usize,
    /// Jobs read in full now.
    read_in_full: usize,
    /// Of those, the good matches kept with the note that their posting does
    /// not say where it hires.
    kept_with_gap: usize,
    /// Why the other jobs read were left out.
    left: JobsLeftOut,
    /// Job sites and company pages the store gave jobs from.
    from_sites: usize,
    from_company_pages: usize,
    /// What the quick looks and full reads were expected to cost, in dollars.
    expected_cost_usd: Option<f64>,
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
            SET status = 'finished', finished_at = $2, added = $3, error_kind = $4,
                kept_with_gap = $5, dropped_by_reason = $6
          WHERE id = $1",
    )
    .bind(row.id)
    .bind(now)
    .bind(i32::try_from(added).unwrap_or(i32::MAX))
    .bind(stopped.map(SearchStop::as_str))
    .bind(i32::try_from(fields.kept_with_gap).unwrap_or(i32::MAX))
    .bind(sqlx::types::Json(&fields.left))
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
        not_grounded = fields.not_grounded,
        batches = fields.batches,
        quick_looked = fields.quick_looked,
        read_in_full = fields.read_in_full,
        kept_with_gap = fields.kept_with_gap,
        left_out = fields.left.total(),
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
    stage: String,
    match_score: i32,
    why: sqlx::types::Json<Vec<String>>,
    gaps: sqlx::types::Json<Vec<String>>,
    read: Option<sqlx::types::Json<Read>>,
    remote: Option<String>,
    open_to_candidate: Option<String>,
    region_note: Option<String>,
    working_languages: Option<Vec<String>>,
    salary: Option<sqlx::types::Json<Salary>>,
    ungrounded: bool,
}

/// What earlier searches gave `account` for these versions of the profile,
/// the goals and the rules, by job key.
#[derive(Debug, Default)]
struct Cache {
    /// Full reads.
    full: HashMap<(String, String), Score>,
    /// Quick looks' fits.
    quick: HashMap<(String, String), u8>,
}

impl Cache {
    /// Whether `job` had a quick look or a full read.
    fn seen(&self, job: &Job) -> bool {
        let key = job.key();
        self.full.contains_key(&key) || self.quick.contains_key(&key)
    }
}

/// Scores already given to `account` for these versions of the profile,
/// the goals and the rules.
async fn cached_scores(db: &PgPool, account: Uuid, input: &MatchInput) -> Result<Cache> {
    let rows: Vec<CachedRow> = sqlx::query_as(
        "SELECT source, source_ref, stage, match_score, why, gaps, read, remote,
                open_to_candidate, region_note, working_languages, salary, ungrounded
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
    let mut cache = Cache::default();
    for row in rows {
        let fit = u8::try_from(row.match_score.clamp(0, 100)).unwrap_or(0);
        let key = (row.source, row.source_ref);
        if row.stage == "quick" {
            cache.quick.insert(key, fit);
            continue;
        }
        let answer = |raw: Option<&str>| raw.map_or(Answer::Unclear, Answer::parse);
        cache.full.insert(
            key,
            Score {
                match_score: fit,
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
                ungrounded: row.ungrounded,
            },
        );
    }
    Ok(cache)
}

/// Keep `look`, the quick look at `job`, for these versions.
async fn save_quick(
    db: &PgPool,
    account: Uuid,
    input: &MatchInput,
    job: &Job,
    look: &Look,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources.scores
            (account_id, source, source_ref, profile_version, goals_version, rules_version,
             stage, match_score, why, gaps, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'quick', $7, $8, '[]'::jsonb, $9)
         ON CONFLICT DO NOTHING",
    )
    .bind(account)
    .bind(job.source_name())
    .bind(&job.source_ref)
    .bind(&input.profile_version)
    .bind(input.goals_version)
    .bind(RULES_VERSION)
    .bind(i32::from(look.fit))
    .bind(sqlx::types::Json(look.reason.iter().collect::<Vec<_>>()))
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
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
             stage, match_score, why, gaps, read, remote, open_to_candidate, region_note,
             working_languages, salary, excluded_reason, created_at, ungrounded)
         VALUES ($1, $2, $3, $4, $5, $6, 'full', $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                 $17, $18)
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
    .bind(score.ungrounded)
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
    /// A page after the first that failed: the pages before it were kept.
    pub partial: Option<PartialRead>,
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
    /// Job sites the store gave jobs from.
    pub from_sites: usize,
    /// Company pages the store gave jobs from.
    pub from_company_pages: usize,
    /// The store's row of each job that came from it, by job key.
    pub ids: HashMap<(String, String), Uuid>,
    /// How close each of those is to the person's roles and skills.
    pub ranks: HashMap<(String, String), f32>,
    /// Jobs of the store the database left out, so the funnel adds up: too
    /// old, not a way of working they want, and past the pool's limit.
    pub outside: Outside,
}

/// Jobs of the store a search did not load, by why.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Outside {
    pub too_old: u32,
    pub not_remote: u32,
    pub not_closest: u32,
}

impl Outside {
    pub(crate) fn total(&self) -> usize {
        [self.too_old, self.not_remote, self.not_closest]
            .into_iter()
            .map(|n| n as usize)
            .sum()
    }
}

/// `n` as a count that cannot be negative or overflow.
fn clamp_u32(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

/// The job sites a search draws from: the person's On sites (every approved
/// site the instance reads, until each person chooses), in the registry's
/// order.
pub(crate) async fn on_sites(
    deps: &SourcesDeps,
    db: &PgPool,
    _account: Uuid,
) -> Result<Vec<BoardRow>> {
    Ok(job_boards::approved(db)
        .await?
        .into_iter()
        .filter(|row| deps.config.is_enabled(&row.slug))
        .collect())
}

/// The words of the person's roles and skills the store's full-text rank
/// looks for in a title, a company and a site's tags: each wanted role's key
/// words (the roles they had when they named none), then the top skills.
pub(crate) fn preselect_terms(input: &MatchInput) -> Vec<String> {
    let wanted = if input.roles.is_empty() {
        &input.titles
    } else {
        &input.roles
    };
    let mut terms: Vec<String> = wanted
        .iter()
        .filter_map(|role| matching::role_words(role))
        .take(5)
        .collect();
    for skill in matching::top_skills(&input.skills) {
        let mut words: Vec<String> = matching::words(skill).into_iter().collect();
        words.sort();
        if !words.is_empty() {
            terms.push(words.join(" "));
        }
    }
    terms
}

/// When `account`'s last finished search before `before` started.
pub(crate) async fn last_search(
    db: &PgPool,
    account: Uuid,
    before: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    Ok(sqlx::query_scalar(
        "SELECT max(started_at) FROM sources.searches
          WHERE account_id = $1 AND status = 'finished' AND started_at < $2",
    )
    .bind(account)
    .bind(before)
    .fetch_one(db)
    .await?)
}

/// Read what the store does not hold: the person's own queries (their roles
/// and top skills) on the sites that search by keyword, and the company
/// boards when they are on. With the store off, the sites are read live in
/// full, as before the store.
pub(crate) async fn read_live(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    rows: &[BoardRow],
    now: DateTime<Utc>,
    fresh_since: DateTime<Utc>,
    queries: &[String],
) -> Result<Reading> {
    let mut reading = Reading::default();
    let mut jobs = Vec::new();
    let reads = if deps.config.store() {
        reader.job_board_queries(db, rows, now, queries).await
    } else {
        reader.job_boards(db, rows, now, fresh_since, queries).await
    };
    for read in reads {
        if read.outcome == Outcome::Error {
            reading.failed += 1;
        }
        reading.boards.push(Scanned {
            read: u32::try_from(read.jobs.len()).unwrap_or(u32::MAX),
            board: read.board,
            outcome: read.outcome,
            error: read.error,
            duration_ms: read.duration_ms,
            partial: read.partial,
        });
        jobs.extend(read.jobs);
    }
    if deps.config.company_boards() {
        let company = boards::all(db).await?;
        let fetched = reader.all(db, &company).await;
        reading.failed += fetched.failed;
        jobs.extend(fetched.jobs);
    }
    reading.jobs = jobs;
    Ok(reading)
}

/// The jobs a search draws from: the store's (the person's On sites and every
/// company page; with `first_seen_since`, only jobs the store first saw
/// after it) and what their own queries find live, once each, the same job
/// on several sites merged. One line per On site for the Scanned view.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn read_sources(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    fresh_since: DateTime<Utc>,
    queries: &[String],
    input: &MatchInput,
    first_seen_since: Option<DateTime<Utc>>,
) -> Result<Reading> {
    let rows = on_sites(deps, db, account).await?;
    let mut reading = read_live(deps, reader, db, &rows, now, fresh_since, queries).await?;
    let mut jobs = std::mem::take(&mut reading.jobs);
    if deps.config.store() {
        let sites: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
        let terms = preselect_terms(input);
        let query = pool_query(deps, input, account, &sites, first_seen_since, &terms);
        // What the database leaves out is counted, not loaded: the pool is
        // its best few thousand (`SEARCH_POOL`), ranked and limited in SQL.
        let counts = store::pool_counts(db, &query, now).await?;
        let pool = store::pool(db, &query, now).await?;
        reading.outside = Outside {
            too_old: clamp_u32(counts.too_old),
            not_remote: clamp_u32(counts.not_wanted),
            not_closest: clamp_u32(counts.passing() - i64::try_from(pool.len()).unwrap_or(0)),
        };
        let mut per_site: HashMap<Uuid, u32> = HashMap::new();
        let mut pages: HashSet<Uuid> = HashSet::new();
        for stored in pool {
            if stored.job.board_kind == "company_page" {
                pages.insert(stored.job.board_id);
            } else {
                add_one(per_site.entry(stored.job.board_id).or_default());
            }
            let (id, rank) = (stored.job.id, stored.rank);
            let job = stored.job.into_job();
            reading.ids.insert(job.key(), id);
            reading.ranks.insert(job.key(), rank);
            jobs.push(job);
        }
        reading.from_sites = per_site.len();
        reading.from_company_pages = pages.len();
        // One line for every On site: what the store gave, and what its own
        // queries gave live.
        let mut lines: Vec<Scanned> = Vec::with_capacity(rows.len());
        for row in &rows {
            let from_store = per_site.get(&row.id).copied().unwrap_or(0);
            match reading.boards.iter().position(|s| s.board.id == row.id) {
                Some(at) => {
                    let mut line = reading.boards.swap_remove(at);
                    line.read = line.read.saturating_add(from_store);
                    lines.push(line);
                }
                None => lines.push(Scanned {
                    board: crate::readers::mapping::BoardRef::of(row),
                    outcome: Outcome::Read,
                    error: None,
                    duration_ms: 0,
                    read: from_store,
                    partial: None,
                }),
            }
        }
        reading.boards = lines;
    }
    reading.jobs = dedupe::merge(unique(jobs));
    Ok(reading)
}

/// What the pool asks the database for.
fn pool_query<'a>(
    deps: &SourcesDeps,
    input: &'a MatchInput,
    account: Uuid,
    sites: &'a [Uuid],
    first_seen_since: Option<DateTime<Utc>>,
    terms: &'a [String],
) -> store::PoolQuery<'a> {
    store::PoolQuery {
        sites,
        first_seen_since,
        text: terms,
        limit: deps.config.search_pool(),
        work_modes: &input.work_modes,
        scored: Some(store::ScoredFor {
            account,
            profile_version: &input.profile_version,
            goals_version: input.goals_version,
            rules_version: RULES_VERSION,
        }),
    }
}

fn add_one(count: &mut u32) {
    *count = count.saturating_add(1);
}

/// Fill in the description of every job of `jobs` that came from the store
/// and does not carry it yet (a pool leaves them out).
async fn hydrate<'a>(
    db: &PgPool,
    ids: &HashMap<(String, String), Uuid>,
    jobs: impl Iterator<Item = &'a mut Job>,
) -> Result<()> {
    let jobs: Vec<&mut Job> = jobs
        .filter(|job| job.description.is_empty() && ids.contains_key(&job.key()))
        .collect();
    let wanted: Vec<Uuid> = jobs
        .iter()
        .filter_map(|job| ids.get(&job.key()).copied())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let found = store::descriptions(db, &wanted).await?;
    for job in jobs {
        if let Some(text) = ids.get(&job.key()).and_then(|id| found.get(id)) {
            job.description.clone_from(text);
        }
    }
    Ok(())
}

/// What one search did on one board.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    passed: u32,
    /// Given a quick look now.
    quick_looked: u32,
    /// Read in full now.
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

/// What a search did with the watched company boards it read.
#[derive(Debug, Default)]
struct Watched {
    /// What the daily check read.
    reading: watch::check::Reading,
    /// What the search did with each watch's new postings.
    tallies: HashMap<Uuid, Tally>,
}

impl Watched {
    /// The tally of the watch `job` is a new posting of, if it is one.
    fn tally(&mut self, job: &Job) -> Option<&mut Tally> {
        let watch = self.reading.of.get(&job.key())?;
        Some(self.tallies.entry(*watch).or_default())
    }
}

/// Keep what search `row` scanned on each job board, and on each watched
/// company's own board.
async fn record_scans(
    deps: &SourcesDeps,
    db: &PgPool,
    row: &SearchRow,
    boards: &[Scanned],
    tallies: &HashMap<String, Tally>,
    watched: &Watched,
    now: DateTime<Utc>,
) -> Result<()> {
    let rate = usd_to_eur(deps);
    let mut rows: Vec<Scan> = boards
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
                quick_looked: t.quick_looked,
                scored_by_ai: t.scored,
                matched: t.matched,
                ai_cost_eur: t.cost_usd * rate,
                error_kind: scanned.error.clone(),
                duration_ms: scanned.duration_ms,
                note: scanned.partial.map(PartialRead::note),
                company: None,
            }
        })
        .collect();
    for read in &watched.reading.reads {
        let t = watched
            .tallies
            .get(&read.watch)
            .copied()
            .unwrap_or_default();
        rows.push(Scan {
            board: uuid::Uuid::nil(),
            company: Some(read.company),
            outcome: read.outcome.as_str(),
            jobs_read: read.listed,
            passed_requirements: t.passed,
            quick_looked: t.quick_looked,
            scored_by_ai: t.scored,
            matched: t.matched,
            ai_cost_eur: t.cost_usd * rate,
            error_kind: read.error.clone(),
            duration_ms: read.duration_ms,
            note: None,
        });
    }
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

/// `cost`, the price of one call about `jobs`, shared by them: each bears its
/// share, split between the boards it was seen on.
fn charge(tallies: &mut HashMap<String, Tally>, watched: &mut Watched, jobs: &[&Job], cost: f64) {
    if jobs.is_empty() {
        return;
    }
    // 25 at most: exact as f64.
    let share = cost / jobs.len() as f64;
    for job in jobs {
        let boards = boards_of(job).len().max(1) as f64;
        for t in tally(tallies, job) {
            t.cost_usd += share / boards;
        }
        if let Some(t) = watched.tally(job) {
            t.cost_usd += share;
        }
    }
}

/// What to do after a model call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterFailure {
    /// Make no more calls: a spending limit, no model, or the provider is
    /// down.
    Stop,
    /// Ask again for each half of the batch.
    Split,
    /// Leave the batch for the next search.
    Skip,
}

/// The search's model calls so far: how many failed in a row, and why it
/// stopped calling, if it did.
#[derive(Debug, Default)]
struct Calls {
    failed_in_a_row: u32,
    stopped: Option<SearchStop>,
}

impl Calls {
    fn answered(&mut self) {
        self.failed_in_a_row = 0;
    }

    /// A call about `jobs` jobs failed with `error`. A batch whose answer is
    /// unusable (cut off, or not the JSON asked for) is asked again as two
    /// halves, and a half that fails too is left for the next search: one
    /// bad answer never ends the search. Only failed calls in a row (the
    /// provider is down) stop it.
    fn failed(
        &mut self,
        error: &applicamp_ai::Error,
        done: &mut Progress,
        jobs: usize,
        may_split: bool,
    ) -> AfterFailure {
        match error {
            applicamp_ai::Error::OverBudget { .. } | applicamp_ai::Error::OverDailyCap { .. } => {
                self.stopped = Some(SearchStop::AiBudgetReached);
                AfterFailure::Stop
            }
            applicamp_ai::Error::NotConfigured(_) => {
                self.stopped = Some(SearchStop::AiUnavailable);
                AfterFailure::Stop
            }
            e => self.unusable(e, done, jobs, may_split),
        }
    }

    /// A call about `jobs` jobs was made and its answer is of no use (`why`):
    /// cut off, not the JSON asked for, or about no job of the batch.
    fn unusable(
        &mut self,
        why: &dyn std::fmt::Display,
        done: &mut Progress,
        jobs: usize,
        may_split: bool,
    ) -> AfterFailure {
        // A call was made: count it.
        done.batches += 1;
        self.failed_in_a_row += 1;
        if self.failed_in_a_row >= FAILED_IN_A_ROW {
            tracing::warn!(error = %why, "model calls keep failing; the search keeps what it has");
            self.stopped = Some(SearchStop::AiFailed);
            AfterFailure::Stop
        } else if may_split && jobs > 1 {
            tracing::warn!(error = %why, jobs, "a model call failed; asking again in halves");
            AfterFailure::Split
        } else {
            tracing::warn!(error = %why, jobs, "a model call failed again; these jobs wait for the next search");
            AfterFailure::Skip
        }
    }
}

/// Why an answer that parsed is still of no use.
const ABOUT_NO_JOB: &str = "the answer is about no job of the batch";

/// A job read in full that is not a match: read now, or judged by an
/// earlier search.
fn not_a_match(left: &mut JobsLeftOut, read_now: bool) {
    add(
        if read_now {
            &mut left.not_good_enough
        } else {
            &mut left.seen_before
        },
        1,
    );
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

    // Read the boards, asking the ones that search for their roles and stack:
    // at night, what the night asks for everyone (read just before, and
    // never more than a board's share); else this person's own.
    let (fresh_since, queries) = if row.trigger == "nightly" {
        let accounts = neighbours.accounts_with_goals(db).await?;
        (
            row.started_at - NIGHTLY_READ,
            night_queries(neighbours.as_ref(), db, &accounts, now).await?,
        )
    } else {
        (row.started_at, input.queries.clone())
    };
    // A nightly search looks only at jobs the store first saw since this
    // person's last search; "Search now" at every job it holds.
    let first_seen_since = if row.trigger == "nightly" {
        last_search(db, account, row.started_at).await?
    } else {
        None
    };
    let reading = read_sources(
        deps,
        reader,
        db,
        account,
        now,
        fresh_since,
        &queries,
        &input,
        first_seen_since,
    )
    .await?;
    let failed = reading.failed;
    let scanned = reading.boards;
    let (store_ids, store_ranks) = (reading.ids, reading.ranks);
    done.from_sites = reading.from_sites;
    done.from_company_pages = reading.from_company_pages;
    let outside = reading.outside;
    done.left.too_old = done.left.too_old.saturating_add(outside.too_old);
    done.left.not_remote = done.left.not_remote.saturating_add(outside.not_remote);
    done.left.preselect = done.left.preselect.saturating_add(outside.not_closest);
    let mut jobs = reading.jobs;
    let mut tallies: HashMap<String, Tally> = scanned
        .iter()
        .map(|s| (s.board.slug.clone(), Tally::default()))
        .collect();
    // The watched companies' own job boards (#20), once a day: only their
    // new postings go on, with the same limits, in the same batches.
    let mut watched = Watched::default();
    if !spent_out(&spend(deps, db, account, now).await?) {
        watched.reading = watch::check::read_due(deps, reader, db, account, now).await?;
    }
    done.fetched = jobs.len() + outside.total() + watched.reading.jobs.len();
    progress(db, id, &done).await?;
    if jobs.is_empty() && watched.reading.jobs.is_empty() && failed > 0 {
        let now = deps.clock.now();
        record_scans(deps, db, &row, &scanned, &tallies, &watched, now).await?;
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

    jobs.extend(watched.reading.jobs.clone());

    // A job already in the pipeline from another board: credit this board
    // on it, and read it no further.
    let known = neighbours.known_jobs(db, account).await?;
    let links = neighbours
        .known_links(db, account, now - DEDUPE_WINDOW)
        .await?;
    let mut attach: Vec<Attach> = Vec::new();
    let read = jobs.len();
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
    add(&mut done.left.seen_before, read - jobs.len());
    if !attach.is_empty() {
        neighbours
            .attach_sources(db, account, attach, deps.clock.now())
            .await?;
    }

    // The hard limits.
    let mut candidates: Vec<Job> = Vec::new();
    for job in jobs {
        match matching::hard_limit(&job, &input, &known, now) {
            None => candidates.push(job),
            Some(out) => done.left.hard_limit(out),
        }
    }
    for job in &candidates {
        for t in tally(&mut tallies, job) {
            t.passed += 1;
        }
        if let Some(t) = watched.tally(job) {
            t.passed += 1;
        }
    }
    done.kept = candidates.len();

    // A job an earlier search read in full is not read again (for these
    // versions of the profile, goals and rules): it never takes a slot. A
    // watched company's new postings are read in full ahead of the cut; the
    // rest wait in the pre-rank's order, the most promising first.
    let cache = cached_scores(db, account, &input).await?;
    let mut scored: Vec<(Job, Score, bool)> = Vec::new();
    let mut to_read: Vec<Job> = Vec::new();
    let (mine, others): (Vec<Job>, Vec<Job>) = candidates
        .into_iter()
        .partition(|job| watched.reading.of.contains_key(&job.key()));
    for job in mine {
        match cache.full.get(&job.key()) {
            Some(score) => scored.push((job, score.clone(), false)),
            None => to_read.push(job),
        }
    }
    let mut unread: Vec<Job> = Vec::new();
    for job in matching::pre_rank(others, &input, now) {
        match cache.full.get(&job.key()) {
            Some(score) => scored.push((job, score.clone(), false)),
            None => unread.push(job),
        }
    }
    done.scored = scored.len();

    // The store can hold tens of thousands of jobs: before any model call,
    // only the closest to the person's roles and skills that have no look
    // yet go on to a quick look (the words of the title, the company and the
    // site's tags, then how new), at most `QUICK_LOOK_PER_SEARCH`. The rest
    // are left for a later search; a job already looked at keeps competing
    // for a full read at no cost.
    let cap = deps.config.quick_look_per_search();
    let mut fresh: Vec<((String, String), f32)> = unread
        .iter()
        .map(Job::key)
        .filter(|key| !cache.quick.contains_key(key))
        .filter_map(|key| store_ranks.get(&key).map(|rank| (key, *rank)))
        .collect();
    fresh.sort_by(|(ka, a), (kb, b)| b.total_cmp(a).then_with(|| ka.cmp(kb)));
    let cut: HashSet<(String, String)> = fresh
        .split_off(fresh.len().min(cap))
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    if !cut.is_empty() {
        unread.retain(|job| !cut.contains(&job.key()));
        add(&mut done.left.preselect, cut.len());
    }
    let chosen: HashSet<(String, String)> = fresh.into_iter().map(|(key, _)| key).collect();
    hydrate(
        db,
        &store_ids,
        unread.iter_mut().filter(|job| chosen.contains(&job.key())),
    )
    .await?;
    progress(db, id, &done).await?;

    let started = std::time::Instant::now();
    let mut calls = Calls::default();
    let slots = deps.config.full_scores_per_search();
    let per_batch = per_batch_usd(deps, db, account).await?;
    if unread.len() <= slots {
        // Every one is read in full: a quick look would change nothing.
        let expected = calls_for(unread.len() + to_read.len(), slots, Some(0)) as f64 * per_batch;
        done.expected_cost_usd = Some(expected);
        to_read.extend(unread);
    } else {
        // A quick look at every one without one, 25 a call, the most
        // promising first; then the best by fit (ties by the pre-rank) are
        // read in full, and the others compete again next time.
        let mut fits: Vec<Option<(u8, bool)>> = unread
            .iter()
            .map(|job| cache.quick.get(&job.key()).map(|fit| (*fit, false)))
            .collect();
        let look_at: Vec<usize> = (0..unread.len()).filter(|i| fits[*i].is_none()).collect();
        // What the quick looks and the full reads are expected to cost, from
        // what a call has cost this person on average.
        let expected =
            calls_for(unread.len() + to_read.len(), slots, Some(look_at.len())) as f64 * per_batch;
        done.expected_cost_usd = Some(expected);
        tracing::info!(
            quick_looks = look_at.len(),
            expected_cost_usd = expected,
            "a search's expected cost"
        );
        let mut queue: std::collections::VecDeque<(&[usize], bool)> = look_at
            .chunks(quick::BATCH)
            .map(|batch| (batch, true))
            .collect();
        while let Some((batch, may_split)) = queue.pop_front() {
            if calls.stopped.is_some() || started.elapsed() > QUICK_LOOK_TIME {
                break;
            }
            let jobs: Vec<&Job> = batch.iter().map(|i| &unread[*i]).collect();
            let caller = Caller {
                db,
                account,
                now: deps.clock.now(),
            };
            let after = match quick::look(ai, caller, &input, &jobs).await {
                // Answered, but about no job of the batch (its refs as
                // numbers, or names it made up): as unusable as a cut-off
                // answer.
                Ok((looks, cost)) if looks.is_empty() => {
                    done.cost_usd += cost;
                    charge(&mut tallies, &mut watched, &jobs, cost);
                    Some(calls.unusable(&ABOUT_NO_JOB, &mut done, batch.len(), may_split))
                }
                Ok((looks, cost)) => {
                    calls.answered();
                    done.batches += 1;
                    done.cost_usd += cost;
                    charge(&mut tallies, &mut watched, &jobs, cost);
                    for (index, look) in looks {
                        let i = batch[index];
                        let job = &unread[i];
                        save_quick(db, account, &input, job, &look, deps.clock.now()).await?;
                        fits[i] = Some((look.fit, true));
                        done.quick_looked += 1;
                        for t in tally(&mut tallies, job) {
                            t.quick_looked += 1;
                        }
                        if let Some(t) = watched.tally(job) {
                            t.quick_looked += 1;
                        }
                    }
                    progress(db, id, &done).await?;
                    None
                }
                Err(e) => Some(calls.failed(&e, &mut done, batch.len(), may_split)),
            };
            match after {
                None | Some(AfterFailure::Skip) => {}
                Some(AfterFailure::Stop) => break,
                Some(AfterFailure::Split) => {
                    let (first, second) = batch.split_at(batch.len() / 2);
                    queue.push_front((second, false));
                    queue.push_front((first, false));
                }
            }
        }
        let mut ranked: Vec<(u8, usize)> = fits
            .iter()
            .enumerate()
            .filter_map(|(i, fit)| fit.map(|(fit, _)| (fit, i)))
            .collect();
        ranked.sort_by(|(fa, ia), (fb, ib)| fb.cmp(fa).then(ia.cmp(ib)));
        let mut unread: Vec<Option<Job>> = unread.into_iter().map(Some).collect();
        for (_, i) in ranked.iter().take(slots) {
            if let Some(job) = unread[*i].take() {
                to_read.push(job);
            }
        }
        for (job, fit) in unread.iter().zip(&fits) {
            if job.is_none() {
                continue;
            }
            let count = match fit {
                // Looked at now, not among the best.
                Some((_, true)) => &mut done.left.quick_look,
                // Looked at by an earlier search, and still not among them.
                Some((_, false)) => &mut done.left.seen_before,
                // Not looked at: a limit, the time or a failed call.
                None => &mut done.left.waiting,
            };
            add(count, 1);
        }
        progress(db, id, &done).await?;
    }

    // Read the chosen ones in full, ten at a time, within the limits.
    hydrate(db, &store_ids, to_read.iter_mut()).await?;
    if calls.stopped.is_none() {
        for job in &mut to_read {
            if let Err(e) = reader.describe(db, job).await {
                tracing::info!(error = %e, "a job's description could not be read");
            }
        }
    }
    let mut read_now = vec![false; to_read.len()];
    let mut queue: std::collections::VecDeque<(std::ops::Range<usize>, bool)> = (0..to_read.len())
        .step_by(BATCH)
        .map(|start| (start..(start + BATCH).min(to_read.len()), true))
        .collect();
    while let Some((range, may_split)) = queue.pop_front() {
        if calls.stopped.is_some() || started.elapsed() > SCORING_TIME {
            break;
        }
        let batch = &to_read[range.clone()];
        let jobs: Vec<&Job> = batch.iter().collect();
        let caller = Caller {
            db,
            account,
            now: deps.clock.now(),
        };
        let after = match scoring::score(ai, caller, &input, &jobs).await {
            Ok((scores, cost)) if scores.is_empty() => {
                done.cost_usd += cost;
                charge(&mut tallies, &mut watched, &jobs, cost);
                Some(calls.unusable(&ABOUT_NO_JOB, &mut done, range.len(), may_split))
            }
            Ok((scores, cost)) => {
                calls.answered();
                done.batches += 1;
                done.cost_usd += cost;
                charge(&mut tallies, &mut watched, &jobs, cost);
                for (index, score) in scores {
                    let job = &batch[index];
                    let excluded =
                        requirements::check(&input, &score.facts, place_of(job, &score)).excluded;
                    save_score(db, account, &input, job, &score, excluded, deps.clock.now())
                        .await?;
                    read_now[range.start + index] = true;
                    done.read_in_full += 1;
                    for t in tally(&mut tallies, job) {
                        t.scored += 1;
                    }
                    if let Some(t) = watched.tally(job) {
                        t.scored += 1;
                    }
                    scored.push((job.clone(), score, true));
                }
                done.scored = scored.len();
                progress(db, id, &done).await?;
                None
            }
            Err(e) => Some(calls.failed(&e, &mut done, range.len(), may_split)),
        };
        match after {
            None | Some(AfterFailure::Skip) => {}
            Some(AfterFailure::Stop) => break,
            Some(AfterFailure::Split) => {
                let middle = range.start + range.len() / 2;
                queue.push_front((middle..range.end, false));
                queue.push_front((range.start..middle, false));
            }
        }
    }
    // What was chosen but not read waits for the next search; a watched
    // posting among them is not noted as seen.
    let mut waiting: HashSet<(String, String)> = HashSet::new();
    for (job, read) in to_read.iter().zip(&read_now) {
        if !read {
            add(&mut done.left.waiting, 1);
            if watched.reading.of.contains_key(&job.key()) {
                waiting.insert(job.key());
            }
        }
    }
    if done.left.waiting > 0 {
        tracing::info!(waiting = done.left.waiting, "jobs left for the next search");
    }
    let stopped = calls.stopped;

    // Keep the good ones that meet every requirement: the rules decide on
    // the facts, never the score.
    // Each with whether it was read now and kept with the note that its
    // posting does not say where it hires.
    let mut keep: Vec<(Scored, bool)> = Vec::new();
    for (job, mut score, read_now) in scored {
        let verdict = requirements::check(&input, &score.facts, place_of(&job, &score));
        if let Some(reason) = verdict.excluded {
            done.dropped += 1;
            if read_now {
                done.left.requirement(reason);
            } else {
                add(&mut done.left.seen_before, 1);
            }
            continue;
        }
        if score.ungrounded {
            done.not_grounded += 1;
        }
        if score.match_score < KEEP {
            not_a_match(&mut done.left, read_now);
            continue;
        }
        let Some(job) = with_reading(job, &score) else {
            not_a_match(&mut done.left, read_now);
            continue;
        };
        // The rules' notes first ("Doesn't say if it hires in Portugal",
        // "Salary not stated"): the pipeline keeps three gaps.
        let mut gaps = verdict.gaps;
        gaps.append(&mut score.gaps);
        score.gaps = gaps;
        let own_board = watched
            .reading
            .of
            .get(&job.key())
            .and_then(|watch| watched.reading.company_of.get(watch))
            .map(|(_, company)| OwnBoard {
                company: company.clone(),
            });
        keep.push((
            Scored {
                job,
                score,
                own_board,
            },
            read_now && verdict.hires_unclear,
        ));
    }
    keep.sort_by_key(|(scored, _)| std::cmp::Reverse(scored.score.match_score));
    // The most one search adds; the rest are added by the next.
    if keep.len() > MAX_ADDED {
        add(&mut done.left.waiting, keep.len() - MAX_ADDED);
        keep.truncate(MAX_ADDED);
    }
    done.kept_with_gap = keep.iter().filter(|(_, noted)| *noted).count();
    let mut keep: Vec<Scored> = keep.into_iter().map(|(scored, _)| scored).collect();
    hydrate(
        db,
        &store_ids,
        keep.iter_mut().map(|scored| &mut scored.job),
    )
    .await?;
    for scored in &keep {
        for t in tally(&mut tallies, &scored.job) {
            t.matched += 1;
        }
        if let Some(t) = watched.tally(&scored.job) {
            t.matched += 1;
        }
    }
    let now = deps.clock.now();
    record_scans(deps, db, &row, &scanned, &tallies, &watched, now).await?;
    let matched = keep.len();
    let added = neighbours.add_found(db, events, account, keep, now).await?;
    // Matches the pipeline already had (the same job under another link).
    add(
        &mut done.left.seen_before,
        matched.saturating_sub(usize::try_from(added).unwrap_or(0)),
    );
    let mut seen: HashMap<Uuid, Vec<(String, String)>> = HashMap::new();
    for (key, watch) in &watched.reading.of {
        if !waiting.contains(key) {
            seen.entry(*watch).or_default().push(key.clone());
        }
    }
    for (watch, keys) in seen {
        watch::check::mark_seen(db, watch, &keys, now).await?;
    }
    let finished = finish(db, events, &row, &done, added, stopped, now).await;
    // New matches may make a company worth watching.
    if finished.is_ok()
        && let Err(e) = watch::suggest::refresh(deps, db, account, now).await
    {
        tracing::warn!(error = %e, "company suggestions could not be worked out");
    }
    finished
}

/// What the boards are asked in a night ([`matching::shared_queries`]): every
/// account's roles and top skills in turn, six at most, the same for the
/// nightly reading and every nightly search it starts.
async fn night_queries(
    neighbours: &dyn crate::Neighbours,
    db: &PgPool,
    accounts: &[Uuid],
    now: DateTime<Utc>,
) -> Result<Vec<String>> {
    let mut each: Vec<Vec<String>> = Vec::new();
    for account in accounts {
        if let Some(input) = neighbours.matching_input(db, *account, now).await? {
            each.push(input.queries);
        }
    }
    Ok(matching::shared_queries(&each))
}

/// The nightly search: every evening at 21:00 Lisbon time, for each account
/// with goals, only when the boards have jobs within their limits that no
/// search looked at or read for them, and never for an account already at a
/// spending limit. Schedules the next one first.
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
        schedule_next_nightly(&ctx.db, deps).await?;
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

/// From inside a nightly search: schedule tomorrow's (this one, running,
/// does not count).
async fn schedule_next_nightly(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.nightly() {
        return Ok(false);
    }
    let at = lisbon::next_run(deps.clock.now());
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_next_at(&mut tx, &Nightly {}, at).await?;
    tx.commit().await?;
    Ok(added)
}

pub(crate) async fn nightly(deps: &SourcesDeps, db: &PgPool) -> Result<()> {
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
    // cache (within each board's polling interval), with the same queries.
    let now = deps.clock.now();
    let queries = night_queries(neighbours.as_ref(), db, &accounts, now).await?;
    // The store has the sites' own feeds already; what is read here is the
    // night's queries, so each search finds them in the cache.
    let sites = on_sites(deps, db, Uuid::nil()).await?;
    let site_ids: Vec<Uuid> = sites.iter().map(|row| row.id).collect();
    let jobs = dedupe::merge(unique(
        read_live(deps, reader, db, &sites, now, now, &queries)
            .await?
            .jobs,
    ));
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
        let cache = cached_scores(db, account, &input).await?;
        // Jobs the store first saw since their last search (the night's
        // search looks at no others), and what their queries found.
        let new: Vec<Job> = if deps.config.store() {
            let since = last_search(db, account, now).await?;
            let terms = preselect_terms(&input);
            let query = pool_query(deps, &input, account, &site_ids, since, &terms);
            store::pool(db, &query, now)
                .await?
                .into_iter()
                .map(|stored| stored.job.into_job())
                .collect()
        } else {
            Vec::new()
        };
        // A job within their limits that no search looked at yet: with no
        // quick look or full read, it always makes the search run.
        let unseen = jobs.iter().chain(&new).any(|job| {
            matching::hard_limit(job, &input, &known, now).is_none() && !cache.seen(job)
        });
        // A watched company's job board is read once a day, with the search.
        let watch_due = watch::store::any_due(db, account, now - deps.config.watch_every()).await?;
        if !unseen && !watch_due {
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
