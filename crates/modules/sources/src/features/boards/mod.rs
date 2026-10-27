//! `/api/v1/boards`: the job boards, ranked by average salary, what each
//! search and discovery run scanned on them, and the owner's decisions.
//!
//! - Everyone signed in sees the boards (with the salary ranking for where
//!   they live), the candidates' checks, and the Scanned view of their own
//!   searches and of discovery; and may suggest a board (it becomes a
//!   candidate, checked at once, never read before it is approved).
//! - The board owners (`APPLICAMP_SOURCES__BOARD_OWNERS`) approve, decline,
//!   pause, edit a board's config and re-run its check. Approving needs a
//!   check of the board's host whose robots rules allow its feed; anyone
//!   else is refused (403 `not_board_owner`).

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    SourcesDeps,
    features::{discovery::CheckBoard, search::SearchTrigger},
    job_boards::{
        self, BoardConfig, BoardRow, Kind, Origin, Priority, Report, Status, Terms, host_of,
    },
    net,
    pay::{self, Ranking},
    regions, robots,
    scans::{self, Totals},
};

/// Suggestions one account may make a day.
pub const SUGGESTIONS_PER_DAY: i64 = 10;
/// Runs per page of the Scanned view.
pub const RUNS_PER_PAGE: i64 = 10;

/// What a board did for the person over some days.
#[derive(Debug, Clone, Copy, Default, Serialize, ToSchema)]
pub struct BoardTotals {
    /// Jobs read by their searches.
    pub read: u32,
    /// Jobs that matched.
    pub matched: u32,
    /// What the AI cost on its jobs, in euros.
    pub cost_eur: f64,
}

impl From<Totals> for BoardTotals {
    fn from(t: Totals) -> Self {
        Self {
            read: t.read,
            matched: t.matched,
            cost_eur: t.cost_eur,
        }
    }
}

/// A job board, as the Boards page lists it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BoardSummary {
    pub id: Uuid,
    /// Its stable name: the source its jobs carry.
    pub slug: String,
    pub name: String,
    pub homepage: String,
    pub kind: Kind,
    /// A label only: the order is the salary ranking.
    pub priority: Priority,
    pub status: Status,
    pub origin: Origin,
    /// The one country its jobs are in, when it is country-scoped.
    pub country: Option<String>,
    /// Its average salary and its key in the order.
    pub ranking: Ranking,
    /// How its jobs are credited ("From Himalayas").
    pub attribution: String,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub last_fetch_jobs: Option<u32>,
    /// What went wrong at its last fetch.
    pub last_error: Option<String>,
    pub decline_reason: Option<String>,
    /// What the owner should know (from its check).
    pub flags: Vec<String>,
    /// Its check's note, for a candidate.
    pub note: Option<String>,
    /// The person's searches on it over the last 7 days.
    pub totals_7d: BoardTotals,
    /// Over the last 30 days.
    pub totals_30d: BoardTotals,
}

/// The job boards, highest average salary first (none last).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BoardList {
    /// The signed-in person manages the boards.
    pub can_manage: bool,
    pub boards: Vec<BoardSummary>,
}

/// A job board, whole: its check, config and terms.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BoardDetail {
    pub board: BoardSummary,
    /// How it is read (the config's JSON).
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
    pub terms: Terms,
    /// What the automatic check found; none before it ran.
    pub report: Option<Report>,
    /// The address its jobs are read from.
    pub endpoint: Option<String>,
    /// It can be approved now; else `blocked` says why.
    pub approvable: bool,
    /// `check_needed`, `robots_disallowed`, `config_invalid`, or none.
    pub blocked: Option<String>,
    pub can_manage: bool,
}

/// Why a board is declined.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclineBoard {
    /// Up to 500 characters.
    pub reason: String,
}

/// A board's new config.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardConfigChange {
    /// The whole config, validated for the board's kind.
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
}

/// A board to suggest.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardSuggestion {
    /// Its site (https).
    pub url: String,
}

/// What the Scanned view lists.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ScansQuery {
    /// Only this board's lines.
    pub board: Option<Uuid>,
    /// Only this search.
    pub search: Option<Uuid>,
    /// The page after this one (`next_cursor`).
    pub cursor: Option<DateTime<Utc>>,
}

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanRunKind {
    Search,
    Discovery,
}

/// How a board's scan went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanOutcome {
    /// Read now.
    Read,
    /// Within its polling interval: its last answer was used.
    SkippedPollLimit,
    /// It could not be read (`error_kind` says why).
    Error,
    /// A discovery check.
    Checked,
}

impl ScanOutcome {
    fn parse(raw: &str) -> Self {
        match raw {
            "read" => Self::Read,
            "skipped_poll_limit" => Self::SkippedPollLimit,
            "checked" => Self::Checked,
            _ => Self::Error,
        }
    }
}

/// One board's line of a run.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanLine {
    pub board_id: Uuid,
    pub board_slug: String,
    pub board_name: String,
    pub outcome: ScanOutcome,
    pub jobs_read: u32,
    /// Within the person's requirements on the boards' structured fields.
    pub passed_requirements: u32,
    /// Scored by the AI (now, or before).
    pub scored_by_ai: u32,
    /// Good matches that meet every requirement.
    pub matched: u32,
    pub ai_cost_eur: f64,
    pub error_kind: Option<String>,
    pub duration_ms: u32,
    /// What a discovery check found.
    pub note: Option<String>,
}

/// A search or a discovery run, and what it scanned on each board.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanRun {
    pub kind: ScanRunKind,
    pub id: Uuid,
    pub at: DateTime<Utc>,
    /// A search's trigger; none for discovery.
    pub trigger: Option<SearchTrigger>,
    /// What its AI calls cost, in euros (discovery uses none).
    pub cost_eur: f64,
    pub lines: Vec<ScanLine>,
}

/// A page of runs, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanPage {
    pub runs: Vec<ScanRun>,
    /// Pass as `cursor` for older runs; none on the last page.
    pub next_cursor: Option<DateTime<Utc>>,
}

fn count(n: Option<i32>) -> Option<u32> {
    n.and_then(|n| u32::try_from(n).ok())
}

fn neighbours(deps: &SourcesDeps) -> Result<&std::sync::Arc<dyn crate::Neighbours>> {
    deps.neighbours
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))
}

/// Whether `account` manages the job boards.
async fn is_owner(deps: &SourcesDeps, db: &PgPool, account: Uuid) -> Result<bool> {
    let email = neighbours(deps)?.account_email(db, account).await?;
    Ok(email.is_some_and(|email| deps.config.is_board_owner(&email)))
}

async fn owner_only(deps: &SourcesDeps, db: &PgPool, account: Uuid) -> Result<()> {
    if is_owner(deps, db, account).await? {
        Ok(())
    } else {
        Err(Error::Forbidden("not_board_owner"))
    }
}

/// The country where `account` lives, for the salary ranking.
async fn home_country(
    deps: &SourcesDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<&'static str>> {
    let input = neighbours(deps)?.matching_input(db, account, now).await?;
    Ok(input
        .and_then(|input| input.home)
        .and_then(|home| regions::country_of(&home)))
}

fn summary(row: &BoardRow, ranking: Ranking, totals: Option<&(Totals, Totals)>) -> BoardSummary {
    let report = row.report();
    let (t7, t30) = totals.copied().unwrap_or_default();
    BoardSummary {
        id: row.id,
        slug: row.slug.clone(),
        name: row.name.clone(),
        homepage: row.homepage.clone(),
        kind: row.kind().unwrap_or(Kind::JsonApi),
        priority: row.priority(),
        status: row.status(),
        origin: row.origin(),
        country: row.country.clone(),
        ranking,
        attribution: row.attribution(),
        last_fetch_at: row.last_fetch_at,
        last_fetch_jobs: count(row.last_fetch_jobs),
        last_error: row.last_error.clone(),
        decline_reason: row.decline_reason.clone(),
        flags: report.as_ref().map(|r| r.flags.clone()).unwrap_or_default(),
        note: report.map(|r| r.notes).filter(|n| !n.trim().is_empty()),
        totals_7d: t7.into(),
        totals_30d: t30.into(),
    }
}

/// Every board's ranking for someone in `home`.
async fn rankings(
    db: &PgPool,
    rows: &[BoardRow],
    home: Option<&str>,
    now: DateTime<Utc>,
) -> Result<HashMap<Uuid, Ranking>> {
    let listed = job_boards::listed(db, now - TimeDelta::days(30)).await?;
    let mut by_board: HashMap<Uuid, Vec<&job_boards::ListedJob>> = HashMap::new();
    for (board, job) in &listed {
        by_board.entry(*board).or_default().push(job);
    }
    Ok(rows
        .iter()
        .map(|row| {
            let jobs = by_board.get(&row.id).map(Vec::as_slice).unwrap_or(&[]);
            (
                row.id,
                pay::rank(jobs, home, row.country.as_deref(), &pay::COUNTRIES),
            )
        })
        .collect())
}

/// Whether `row` can be approved now; why not if not.
fn approvable(row: &BoardRow) -> std::result::Result<(), &'static str> {
    let Some(kind) = row.kind() else {
        return Err("config_invalid");
    };
    let Ok(config) = row.config() else {
        return Err("config_invalid");
    };
    if config.validate(kind).is_err() {
        return Err("config_invalid");
    }
    if kind == Kind::Hn {
        return Ok(());
    }
    let Some(start) = config.start(kind) else {
        return Err("config_invalid");
    };
    let report = row.report().unwrap_or_default();
    let checked_host = report.robots.host.as_deref();
    if report.checked_at.is_none() || checked_host != host_of(start).as_deref() {
        return Err("check_needed");
    }
    if robots::allowed(&report.robots.rules, &robots::path_of(start)).0 {
        Ok(())
    } else {
        Err("robots_disallowed")
    }
}

async fn detail(
    deps: &SourcesDeps,
    state: &AppState,
    account: Uuid,
    row: BoardRow,
) -> Result<BoardDetail> {
    let now = state.clock.now();
    let home = home_country(deps, &state.db, account, now).await?;
    let ranking = rankings(&state.db, std::slice::from_ref(&row), home, now)
        .await?
        .remove(&row.id)
        .unwrap_or_else(|| pay::rank(&[], home, row.country.as_deref(), &pay::COUNTRIES));
    let totals = scans::totals(&state.db, account, now).await?;
    let config = row.config().ok();
    let blocked = approvable(&row).err().map(str::to_owned);
    Ok(BoardDetail {
        board: summary(&row, ranking, totals.get(&row.id)),
        endpoint: config
            .as_ref()
            .and_then(|c| row.kind().and_then(|k| c.start(k)))
            .map(Url::to_string),
        config: row.config.0.clone(),
        terms: row.terms(),
        report: row.report(),
        approvable: blocked.is_none() && matches!(row.status(), Status::Candidate | Status::Paused),
        blocked,
        can_manage: is_owner(deps, &state.db, account).await?,
    })
}

async fn fetch(db: &PgPool, id: Uuid) -> Result<BoardRow> {
    job_boards::get(db, id).await?.ok_or(Error::NotFound)
}

/// The job boards.
///
/// Every board, approved, waiting for the owner, paused or declined,
/// highest average salary first: the mean of the stated salaries of its
/// engineering jobs open to where the person lives (30 days), in euros a
/// year; a country-scoped board with fewer than 10 by its country's typical
/// pay; no salary data last. With the person's totals per board (7 and 30
/// days) and whether they manage the boards.
#[utoipa::path(
    get,
    path = "/api/v1/boards",
    tag = "sources",
    operation_id = "sources.boards",
    security(("session" = [])),
    responses(
        (status = 200, description = "The boards, ranked.", body = BoardList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<BoardList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let rows = job_boards::all(&state.db).await?;
    let home = home_country(&deps, &state.db, account, now).await?;
    let mut rankings = rankings(&state.db, &rows, home, now).await?;
    let totals = scans::totals(&state.db, account, now).await?;
    let mut boards: Vec<BoardSummary> = rows
        .iter()
        .map(|row| {
            let ranking = rankings
                .remove(&row.id)
                .unwrap_or_else(|| pay::rank(&[], home, row.country.as_deref(), &pay::COUNTRIES));
            summary(row, ranking, totals.get(&row.id))
        })
        .collect();
    boards
        .sort_by(|a, b| pay::order(a.ranking.key, b.ranking.key).then_with(|| a.name.cmp(&b.name)));
    Ok(Json(BoardList {
        can_manage: is_owner(&deps, &state.db, account).await?,
        boards,
    }))
}

/// A job board.
///
/// Its ranking, config, terms and the automatic check (robots rules, the
/// terms' sentences, sample jobs, the proposed attribution), and whether it
/// can be approved now.
#[utoipa::path(
    get,
    path = "/api/v1/boards/{id}",
    tag = "sources",
    operation_id = "sources.board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "The board.", body = BoardDetail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<BoardDetail>> {
    let row = fetch(&state.db, id).await?;
    Ok(Json(
        detail(&deps, &state, principal.account_id, row).await?,
    ))
}

async fn set_status(
    db: &PgPool,
    id: Uuid,
    status: Status,
    reason: Option<&str>,
    by: Uuid,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE sources.job_boards
            SET status = $2, decline_reason = $3,
                approved_at = CASE WHEN $2 = 'approved' THEN $5 ELSE approved_at END,
                approved_by = CASE WHEN $2 = 'approved' THEN $4 ELSE approved_by END,
                recheck_after = CASE WHEN $2 = 'approved' THEN $5 + interval '182 days'
                                     ELSE recheck_after END,
                updated_at = $5
          WHERE id = $1",
    )
    .bind(id)
    .bind(status.as_str())
    .bind(reason)
    .bind(by)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// Approve a job board.
///
/// From then on searches read it. Only a candidate or a paused board whose
/// config reads its kind and whose check (of the feed's host) found robots
/// rules that allow the feed. Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/approve",
    tag = "sources",
    operation_id = "sources.approve_board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "Approved.", body = BoardDetail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`not_waiting`: it is approved or declined; \
            `config_invalid`; `check_needed`: check its feed's host first; \
            `robots_disallowed`: its robots rules forbid the feed (edit the config to an allowed \
            path)", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn approve(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_only(&deps, &state.db, account).await?;
    let row = fetch(&state.db, id).await?;
    if !matches!(row.status(), Status::Candidate | Status::Paused) {
        return Err(Error::Conflict(
            "not_waiting",
            "Only a board waiting for you, or paused, can be approved.".to_owned(),
        ));
    }
    if let Err(code) = approvable(&row) {
        let detail = match code {
            "check_needed" => {
                "Check the board first: its robots rules for the feed's host are \
                               not known yet."
            }
            "robots_disallowed" => {
                "The board's robots rules forbid reading its feed. Edit the \
                                    config to an allowed path."
            }
            _ => "The board's config does not read its kind. Edit it first.",
        };
        return Err(Error::Conflict(code, detail.to_owned()));
    }
    let now = state.clock.now();
    set_status(&state.db, id, Status::Approved, None, account, now).await?;
    tracing::info!(board = %row.slug, "job board approved");
    let row = fetch(&state.db, id).await?;
    Ok(Json(detail(&deps, &state, account, row).await?))
}

/// Decline a job board.
///
/// It is never read, and says why. Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/decline",
    tag = "sources",
    operation_id = "sources.decline_board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    request_body = DeclineBoard,
    responses(
        (status = 200, description = "Declined.", body = BoardDetail),
        (status = 400, description = "`invalid_request`: a reason of 1 to 500 characters",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn decline(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<DeclineBoard>, JsonRejection>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_only(&deps, &state.db, account).await?;
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(Error::BadRequest(
            "invalid_request",
            "Say why, in up to 500 characters.".to_owned(),
        ));
    }
    let row = fetch(&state.db, id).await?;
    set_status(
        &state.db,
        id,
        Status::Declined,
        Some(reason),
        account,
        state.clock.now(),
    )
    .await?;
    tracing::info!(board = %row.slug, "job board declined");
    let row = fetch(&state.db, id).await?;
    Ok(Json(detail(&deps, &state, account, row).await?))
}

/// Pause a job board.
///
/// Searches stop reading it until it is approved again. Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/pause",
    tag = "sources",
    operation_id = "sources.pause_board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "Paused.", body = BoardDetail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`not_approved`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn pause(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_only(&deps, &state.db, account).await?;
    let row = fetch(&state.db, id).await?;
    if row.status() != Status::Approved {
        return Err(Error::Conflict(
            "not_approved",
            "Only an approved board can be paused.".to_owned(),
        ));
    }
    sqlx::query("UPDATE sources.job_boards SET status = 'paused', updated_at = $2 WHERE id = $1")
        .bind(id)
        .bind(state.clock.now())
        .execute(&state.db)
        .await?;
    tracing::info!(board = %row.slug, "job board paused");
    let row = fetch(&state.db, id).await?;
    Ok(Json(detail(&deps, &state, account, row).await?))
}

/// Change a job board's config.
///
/// The whole config, validated for the board's kind (unknown keys refused).
/// An approved board's new feed must still be allowed by the robots rules
/// its check found. The seed list never overwrites it again. Board owners
/// only.
#[utoipa::path(
    put,
    path = "/api/v1/boards/{id}/config",
    tag = "sources",
    operation_id = "sources.set_board_config",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    request_body = BoardConfigChange,
    responses(
        (status = 200, description = "Changed.", body = BoardDetail),
        (status = 400, description = "`invalid_config`: the detail says what is wrong",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`robots_disallowed`: an approved board's new feed is not \
            allowed by its robots rules", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn set_config(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<BoardConfigChange>, JsonRejection>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_only(&deps, &state.db, account).await?;
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_config", rejection.body_text()))?;
    let row = fetch(&state.db, id).await?;
    let kind = row
        .kind()
        .ok_or_else(|| Error::internal("a board of an unknown kind"))?;
    let config: BoardConfig = serde_json::from_value(body.config)
        .map_err(|e| Error::BadRequest("invalid_config", e.to_string()))?;
    config
        .validate(kind)
        .map_err(|e| Error::BadRequest("invalid_config", e))?;
    let value = serde_json::to_value(&config).map_err(Error::internal)?;
    if row.status() == Status::Approved {
        let changed = BoardRow {
            config: sqlx::types::Json(value.clone()),
            ..row.clone()
        };
        if approvable(&changed) == Err("robots_disallowed") {
            return Err(Error::Conflict(
                "robots_disallowed",
                "The board's robots rules forbid that feed.".to_owned(),
            ));
        }
    }
    sqlx::query(
        "UPDATE sources.job_boards SET config = $2, config_edited = true, updated_at = $3
          WHERE id = $1",
    )
    .bind(id)
    .bind(sqlx::types::Json(&value))
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    tracing::info!(board = %row.slug, "job board config changed");
    let row = fetch(&state.db, id).await?;
    Ok(Json(detail(&deps, &state, account, row).await?))
}

/// Check a job board again.
///
/// Reads its robots rules (for its feed's host), terms page and a sample of
/// its jobs, once each, in the background; the board shows the result.
/// Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/check",
    tag = "sources",
    operation_id = "sources.check_board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 202, description = "The check runs in the background.", body = BoardDetail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn check(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<BoardDetail>)> {
    let account = principal.account_id;
    owner_only(&deps, &state.db, account).await?;
    let row = fetch(&state.db, id).await?;
    let mut tx = state.db.begin().await?;
    jobs::enqueue(&mut tx, &CheckBoard { board: id }).await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(detail(&deps, &state, account, row).await?),
    ))
}

/// Suggest a job board.
///
/// Its site becomes a candidate, checked in the background (robots rules,
/// terms, a feed, a sample), and waits for the owner: nothing is read for
/// jobs before it is approved. At most 10 a day. The same site twice
/// answers the board already there (200).
#[utoipa::path(
    post,
    path = "/api/v1/boards/suggestions",
    tag = "sources",
    operation_id = "sources.suggest_board",
    security(("session" = [])),
    request_body = BoardSuggestion,
    responses(
        (status = 202, description = "Added as a candidate; the check runs.", body = BoardDetail),
        (status = 200, description = "That site is already a board.", body = BoardDetail),
        (status = 400, description = "`invalid_url`: a public https site", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`suggestion_limit_reached`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn suggest(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    body: std::result::Result<Json<BoardSuggestion>, JsonRejection>,
) -> Result<(StatusCode, Json<BoardDetail>)> {
    let account = principal.account_id;
    let now = state.clock.now();
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let invalid = || {
        Error::BadRequest(
            "invalid_url",
            "Paste the board's address, starting with https://.".to_owned(),
        )
    };
    let raw = body.url.trim();
    if raw.is_empty() || raw.chars().count() > 500 {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    net::check(&url, deps.config.allow_private_hosts()).map_err(|_| invalid())?;
    let host = host_of(&url).ok_or_else(invalid)?;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM sources.job_boards WHERE host = $1")
            .bind(&host)
            .fetch_optional(&state.db)
            .await?;
    if let Some(id) = existing {
        let row = fetch(&state.db, id).await?;
        return Ok((
            StatusCode::OK,
            Json(detail(&deps, &state, account, row).await?),
        ));
    }
    let today: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sources.job_boards WHERE suggested_by = $1 AND created_at > $2",
    )
    .bind(account)
    .bind(now - TimeDelta::days(1))
    .fetch_one(&state.db)
    .await?;
    if today >= SUGGESTIONS_PER_DAY {
        return Err(Error::TooManyRequests(
            "suggestion_limit_reached",
            "That's 10 suggestions today. Try again tomorrow.".to_owned(),
        ));
    }
    let mut homepage = url.clone();
    homepage.set_path("/");
    homepage.set_query(None);
    homepage.set_fragment(None);
    let slug = unique_slug(&state.db, &host).await?;
    let id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO sources.job_boards
            (id, slug, name, homepage, host, kind, priority, status, origin, suggested_by,
             created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'json_api', 'low', 'candidate', 'suggested', $6, $7, $7)",
    )
    .bind(id)
    .bind(&slug)
    .bind(host.chars().take(100).collect::<String>())
    .bind(homepage.as_str())
    .bind(&host)
    .bind(account)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue(&mut tx, &CheckBoard { board: id }).await?;
    tx.commit().await?;
    tracing::info!(board = %slug, "job board suggested");
    let row = fetch(&state.db, id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(detail(&deps, &state, account, row).await?),
    ))
}

/// A slug for a new board on `host` that no board has yet.
pub(crate) async fn unique_slug(db: &PgPool, host: &str) -> Result<String> {
    let base = job_boards::slug_of(host).unwrap_or_else(|| "board".to_owned());
    let base: String = base.chars().take(34).collect();
    let base = base.trim_end_matches('-').to_owned();
    for n in 0..100 {
        let slug = if n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sources.job_boards WHERE slug = $1)")
                .bind(&slug)
                .fetch_one(db)
                .await?;
        if !taken {
            return Ok(slug);
        }
    }
    Ok(format!("board-{}", Uuid::now_v7().simple())
        .chars()
        .take(40)
        .collect())
}

/// What was scanned.
///
/// The person's searches and the discovery runs, newest first, ten a page,
/// each with what it scanned on every board: jobs read, passed their
/// requirements, scored by the AI, matched, the AI's cost, or why a board
/// was skipped (its polling limit) or failed; and what a discovery check
/// found. Filter by board or by search.
#[utoipa::path(
    get,
    path = "/api/v1/boards/scans",
    tag = "sources",
    operation_id = "sources.scans",
    security(("session" = [])),
    params(ScansQuery),
    responses(
        (status = 200, description = "A page of runs.", body = ScanPage),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn scanned(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    query: std::result::Result<Query<ScansQuery>, QueryRejection>,
) -> Result<Json<ScanPage>> {
    let Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    let (runs, rows) = scans::runs(
        &state.db,
        principal.account_id,
        query.board,
        query.search,
        query.cursor,
        RUNS_PER_PAGE + 1,
    )
    .await?;
    let more = runs.len() > usize::try_from(RUNS_PER_PAGE).unwrap_or(10);
    let rate = deps.ai.as_ref().map_or(
        applicamp_ai::DEFAULT_USD_TO_EUR,
        applicamp_ai::Gateway::usd_to_eur,
    );
    let n = |v: i32| u32::try_from(v).unwrap_or(0);
    let mut page: Vec<ScanRun> = runs
        .into_iter()
        .take(usize::try_from(RUNS_PER_PAGE).unwrap_or(10))
        .map(|run| ScanRun {
            kind: if run.kind == "discovery" {
                ScanRunKind::Discovery
            } else {
                ScanRunKind::Search
            },
            trigger: match run.trigger.as_str() {
                "manual" => Some(SearchTrigger::Manual),
                "nightly" => Some(SearchTrigger::Nightly),
                _ => None,
            },
            id: run.id,
            at: run.at,
            cost_eur: run.cost_usd * rate,
            lines: Vec::new(),
        })
        .collect();
    for row in rows {
        if let Some(run) = page.iter_mut().find(|r| r.id == row.run_id) {
            run.lines.push(ScanLine {
                board_id: row.board_id,
                board_slug: row.board_slug,
                board_name: row.board_name,
                outcome: ScanOutcome::parse(&row.outcome),
                jobs_read: n(row.jobs_read),
                passed_requirements: n(row.passed_requirements),
                scored_by_ai: n(row.scored_by_ai),
                matched: n(row.matched),
                ai_cost_eur: row.ai_cost_eur,
                error_kind: row.error_kind,
                duration_ms: n(row.duration_ms),
                note: row.note,
            });
        }
    }
    let next_cursor = if more {
        page.last().map(|r| r.at)
    } else {
        None
    };
    Ok(Json(ScanPage {
        runs: page,
        next_cursor,
    }))
}
