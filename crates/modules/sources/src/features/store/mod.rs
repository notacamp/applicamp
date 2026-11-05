//! The job store (#89): every approved job board and company page is read on
//! its own schedule, and what it lists is kept in `sources.store_jobs`.
//!
//! - **A tick every minute** (`sources.read_store`) takes the registry rows
//!   that are due, never-read ones first and job boards before company pages,
//!   at most `APPLICAMP_SOURCES__STORE_READS_PER_MINUTE` of them. Only
//!   **approved** rows are read (a company page also needs its hiring system
//!   approved and its robots rules to allow reading), and only the ones
//!   `APPLICAMP_SOURCES__ENABLED` lets through.
//! - **A host is read one request at a time**: the rows of one host are read
//!   one after the other, up to `APPLICAMP_SOURCES__STORE_PARALLEL` hosts at
//!   once, and the reader keeps the pace between two requests to a host
//!   (`APPLICAMP_SOURCES__REQUESTS_PER_SECOND`, the board's own
//!   `min_interval_ms`). A tick starts no read after 45 seconds: the rest stay
//!   due for the next one.
//! - **Each site's own limits come first**: never before its `poll_minutes`
//!   (searches that still read it live count too: both write
//!   `last_fetch_at`), never faster than its terms' `max_fetches_per_day`,
//!   never before a `Retry-After` it gave, never faster than
//!   `APPLICAMP_SOURCES__STORE_MIN_MINUTES`, and a company page at most once a
//!   day. The robots rules and terms checks are the ones that approved the
//!   row; a JSON-LD site is read only on paths its rules allow, as in a search.
//! - **Conditional requests**: the first request of a read carries the
//!   `ETag` / `Last-Modified` the site gave last time; a 304 keeps every job
//!   as it is and costs no parsing. Only new or changed jobs are written.
//! - **Closing**: a read that listed everything the site has closes the jobs
//!   it no longer lists (a read stopped by a page limit, or through a
//!   sitemap, closes nothing; nor one that would close most of a site's jobs
//!   at once, taken for a partial answer). Closed jobs are deleted after
//!   `APPLICAMP_SOURCES__STORE_CLOSED_DAYS` (14), jobs past their expiry
//!   are dropped, and so is every job no read has seen for 30 days.
//! - **Adaptive cadence**: a site whose reads bring nothing new several times
//!   in a row is read less often (twice, four times... up to a day, a week for
//!   a company page), never more often than its limits allow.
//! - **Apply links** of every read go to the company pages' registration
//!   ([`crate::company_pages::note_links`]), as for a search's read.
//! - **No model is called** anywhere here.
//!
//! [`find`] is what a search will read (the second part of #89);
//! `GET /api/v1/boards/store` ([`totals`]) tells the owner how it goes.

mod keep;
mod read;
#[cfg(test)]
mod tests;

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

use applicamp_kernel::{AppState, JobContext, JobError, Problem, Result, auth::SignedIn, jobs};
use axum::{Extension, Json, extract::State};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    SourcesConfig, SourcesDeps,
    features::boards::owner_only,
    job_boards::{self, BoardRow, Kind, board_columns},
    readers::{BACKOFF, FetchError, Probe, Reader, Validators},
};

pub use keep::{Cleaned, Kept, Limits, clean};
pub use read::{
    MAX_LIMIT, POOL_CEILING, POOL_DEFAULT, PoolCounts, PoolJob, PoolQuery, ScoredFor, StoreQuery,
    StoredJob, descriptions, find, pool, pool_counts, text_query, unscored,
};

/// The time between two ticks.
pub const TICK_EVERY: TimeDelta = TimeDelta::seconds(60);
/// A tick starts no read after this long.
const TICK_BUDGET: Duration = Duration::from_secs(45);
/// How often the store is cleaned, in seconds.
const CLEAN_EVERY: i64 = 3_600;
/// The last cleaning, as a Unix time.
static LAST_CLEAN: AtomicI64 = AtomicI64::new(0);

/// The reading of the approved sites, every minute: starts the reads that are
/// due. Schedules the next tick first, so a failure never stops the reading.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadStore {}

impl applicamp_kernel::Job for ReadStore {
    const NAME: &'static str = "sources.read_store";
    const QUEUE: &'static str = "store";
    // Never retried: the next tick takes what is left.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: Duration = Duration::from_secs(5 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_next(&ctx.db, deps).await?;
        run_tick(deps, &ctx.db)
            .await
            .map(|_| ())
            .map_err(JobError::from)
    }
}

/// Make sure the tick is scheduled, when the store is on: one at every start.
/// Whether one was added or moved.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.store() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added =
        jobs::enqueue_unique_at_or_earlier(&mut tx, &ReadStore {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// From inside a tick: schedule the next.
async fn schedule_next(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.store() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added =
        jobs::enqueue_next_at(&mut tx, &ReadStore {}, deps.clock.now() + TICK_EVERY).await?;
    tx.commit().await?;
    Ok(added)
}

// ---- When a site is read ---------------------------------------------------

/// The least time between two reads of `row`: its own polling limit, the
/// time its terms' fetches a day leave, the instance's floor, and a day for a
/// company page.
pub fn base_interval(row: &BoardRow, config: &SourcesConfig) -> TimeDelta {
    let poll = row
        .config()
        .map_or(TimeDelta::hours(1), |config| config.poll());
    let mut base = poll.max(config.store_min_interval());
    if let Some(per_day) = row.terms().max_fetches_per_day.filter(|n| *n > 0) {
        let minutes = (24 * 60_u32).div_ceil(per_day.min(24 * 60));
        base = base.max(TimeDelta::minutes(i64::from(minutes)));
    }
    if row.kind() == Some(Kind::CompanyPage) {
        base = base.max(TimeDelta::days(1));
    }
    base
}

/// When to read a site again after `quiet` reads in a row that brought
/// nothing new: its `base` interval, doubled as it stays quiet (2, 4, 8, 16
/// times), up to a day (a week for a company page); never less than `base`.
pub fn next_interval(base: TimeDelta, quiet: u32, company_page: bool) -> TimeDelta {
    let times: i32 = match quiet {
        0..=2 => 1,
        3..=5 => 2,
        6..=9 => 4,
        10..=19 => 8,
        _ => 16,
    };
    let most = if company_page {
        TimeDelta::days(7)
    } else {
        TimeDelta::days(1)
    }
    .max(base);
    (base * times).clamp(base, most)
}

// ---- A tick ----------------------------------------------------------------

/// What a tick did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Rows that were due.
    pub due: u32,
    /// Reads that listed jobs.
    pub read: u32,
    /// Reads the site answered with "nothing changed".
    pub not_modified: u32,
    /// Reads that failed.
    pub failed: u32,
    /// Jobs stored for the first time.
    pub new_jobs: u32,
    /// Jobs closed.
    pub closed_jobs: u32,
}

impl Tick {
    fn add(&mut self, done: &Done) {
        match done.status {
            "not_modified" => self.not_modified += 1,
            "error" => self.failed += 1,
            _ => self.read += 1,
        }
        self.new_jobs += done.kept.new;
        self.closed_jobs += done.kept.closed;
    }
}

/// One read's result.
struct Done {
    status: &'static str,
    kept: Kept,
}

/// The rows due at `now`, never-read ones first and job boards before company
/// pages: at most `limit`.
async fn due(
    db: &PgPool,
    now: DateTime<Utc>,
    enabled: Option<&[String]>,
    limit: i64,
) -> Result<Vec<BoardRow>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT b.id
           FROM sources.job_boards b
           LEFT JOIN sources.job_boards s ON s.id = b.system_id
           LEFT JOIN LATERAL (SELECT r.next_read_at FROM sources.store_reads r
                               WHERE r.board_id = b.id
                               ORDER BY r.at DESC, r.id DESC LIMIT 1) prev ON true
          WHERE b.status = 'approved'
            AND (b.kind IN ('json_api', 'rss', 'jsonld_sitemap', 'hn')
                 OR (b.kind = 'company_page' AND s.status = 'approved'
                     AND NOT (s.blockers @> '[{\"code\": \"robots_disallowed\"}]'::jsonb)))
            AND ($3::text[] IS NULL OR b.slug = ANY($3)
                 OR (b.kind = 'company_page' AND b.system = ANY($3)))
            AND coalesce(prev.next_read_at, '-infinity'::timestamptz) <= $1
            AND coalesce(b.backoff_until, '-infinity'::timestamptz) <= $1
            AND (b.last_fetch_at IS NULL
                 OR b.last_fetch_at + make_interval(mins => greatest(15, least(10080,
                        CASE WHEN b.config->>'poll_minutes' ~ '^[0-9]{1,6}$'
                             THEN (b.config->>'poll_minutes')::int ELSE 60 END))) <= $1)
          ORDER BY (b.kind = 'company_page'), prev.next_read_at NULLS FIRST, b.created_at, b.id
          LIMIT $2",
    )
    .bind(now)
    .bind(limit)
    .bind(enabled.map(<[String]>::to_vec))
    .fetch_all(db)
    .await?;
    let mut rows: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards WHERE id = ANY($1)"
    ))
    .bind(&ids)
    .fetch_all(db)
    .await?;
    rows.sort_by_key(|row| ids.iter().position(|id| *id == row.id));
    Ok(rows)
}

/// Start the reads that are due now.
pub async fn run_tick(deps: &SourcesDeps, db: &PgPool) -> Result<Tick> {
    let Some(reader) = deps.reader.clone() else {
        return Ok(Tick::default());
    };
    if !deps.config.store() {
        return Ok(Tick::default());
    }
    let now = deps.clock.now();
    clean_sometimes(deps, db, now).await;
    let limit = i64::try_from(deps.config.store_reads_per_minute()).unwrap_or(60);
    let rows = due(db, now, deps.config.enabled.as_deref(), limit).await?;
    let mut tick = Tick {
        due: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        ..Tick::default()
    };
    // One host at a time: its rows one after the other.
    let mut hosts: Vec<(String, Vec<BoardRow>)> = Vec::new();
    for row in rows {
        match hosts.iter_mut().find(|(host, _)| *host == row.host) {
            Some((_, rows)) => rows.push(row),
            None => hosts.push((row.host.clone(), vec![row])),
        }
    }
    let started = Instant::now();
    let permits = Arc::new(tokio::sync::Semaphore::new(deps.config.store_parallel()));
    let mut reading = tokio::task::JoinSet::new();
    for (_, rows) in hosts {
        let (deps, db, reader, permits) =
            (deps.clone(), db.clone(), reader.clone(), permits.clone());
        reading.spawn(async move {
            let _turn = permits.acquire_owned().await.ok();
            let mut part = Tick::default();
            for row in rows {
                if started.elapsed() > TICK_BUDGET {
                    break;
                }
                match read_one(&deps, &reader, &db, &row, now).await {
                    Ok(done) => part.add(&done),
                    Err(e) => {
                        part.failed += 1;
                        tracing::warn!(board = %row.slug, error = %e, "a read could not be kept");
                    }
                }
            }
            part
        });
    }
    while let Some(part) = reading.join_next().await {
        match part {
            Ok(part) => {
                tick.read += part.read;
                tick.not_modified += part.not_modified;
                tick.failed += part.failed;
                tick.new_jobs += part.new_jobs;
                tick.closed_jobs += part.closed_jobs;
            }
            Err(e) => tracing::warn!(error = %e, "a read stopped"),
        }
    }
    tracing::info!(
        due = tick.due,
        read = tick.read,
        not_modified = tick.not_modified,
        failed = tick.failed,
        new_jobs = tick.new_jobs,
        closed_jobs = tick.closed_jobs,
        "job store tick"
    );
    Ok(tick)
}

/// Clean the store, at most once an hour.
async fn clean_sometimes(deps: &SourcesDeps, db: &PgPool, now: DateTime<Utc>) {
    let at = now.timestamp();
    if at - LAST_CLEAN.load(Ordering::Relaxed) < CLEAN_EVERY {
        return;
    }
    LAST_CLEAN.store(at, Ordering::Relaxed);
    match clean(
        db,
        now,
        deps.config.store_closed_for(),
        deps.config.store_max_total(),
    )
    .await
    {
        Ok(cleaned) => tracing::info!(
            expired = cleaned.expired,
            closed = cleaned.closed,
            stale = cleaned.stale,
            over = cleaned.over,
            reads = cleaned.reads,
            "job store cleaned"
        ),
        Err(e) => tracing::warn!(error = %e, "the job store could not be cleaned"),
    }
}

/// What the latest read of a site left: its schedule and its validators.
#[derive(Debug, sqlx::FromRow)]
struct Before {
    listed: i32,
    quiet: i32,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// The latest read of `board`, among those with `statuses`.
async fn latest(db: &PgPool, board: Uuid, statuses: &[&str]) -> Result<Option<Before>> {
    Ok(sqlx::query_as(
        "SELECT listed, quiet, etag, last_modified FROM sources.store_reads
          WHERE board_id = $1 AND status = ANY($2)
          ORDER BY at DESC, id DESC LIMIT 1",
    )
    .bind(board)
    .bind(statuses)
    .fetch_optional(db)
    .await?)
}

/// What one read says, to be kept as a line of the log.
struct Line<'a> {
    status: &'a str,
    kept: Kept,
    listed: u32,
    http_status: Option<u16>,
    validators: &'a Validators,
    error: Option<&'a str>,
    quiet: u32,
    duration_ms: u32,
    next_read_at: DateTime<Utc>,
}

async fn record(db: &PgPool, board: Uuid, now: DateTime<Utc>, line: &Line<'_>) -> Result<()> {
    let n = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    sqlx::query(
        "INSERT INTO sources.store_reads
            (id, board_id, at, status, listed, new_jobs, changed_jobs, closed_jobs, dropped_jobs,
             http_status, etag, last_modified, error_kind, quiet, duration_ms, next_read_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(Uuid::now_v7())
    .bind(board)
    .bind(now)
    .bind(line.status)
    .bind(n(line.listed))
    .bind(n(line.kept.new))
    .bind(n(line.kept.changed))
    .bind(n(line.kept.closed))
    .bind(n(line.kept.dropped))
    .bind(line.http_status.map(i32::from))
    .bind(&line.validators.etag)
    .bind(&line.validators.last_modified)
    .bind(line.error)
    .bind(n(line.quiet))
    .bind(n(line.duration_ms))
    .bind(line.next_read_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Read `row` once and keep what it lists, its answer and its next time.
async fn read_one(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    now: DateTime<Utc>,
) -> Result<Done> {
    let config = &deps.config;
    let page = row.kind() == Some(Kind::CompanyPage);
    let base = base_interval(row, config);
    let last = latest(db, row.id, &["ok", "not_modified", "partial", "error"]).await?;
    let good = latest(db, row.id, &["ok", "not_modified", "partial"]).await?;
    // Ask whether it changed only when the store still holds what it listed.
    let holds: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sources.store_jobs WHERE board_id = $1)")
            .bind(row.id)
            .fetch_one(db)
            .await?;
    let validators = match &good {
        Some(good) if good.listed == 0 || holds => Validators {
            etag: good.etag.clone(),
            last_modified: good.last_modified.clone(),
        },
        _ => Validators::default(),
    };
    let quiet_before = last
        .as_ref()
        .map_or(0, |last| u32::try_from(last.quiet).unwrap_or(0));
    let probe = Probe::new(validators);
    let started = Instant::now();
    let result = reader.store_read(db, row, now, &probe).await;
    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    let seen = probe.seen();
    match result {
        Ok(jobs) => {
            if !page {
                crate::company_pages::note_links(db, &jobs, &row.name, now).await;
            }
            let full = !probe.partial();
            let tags: String = row
                .tags
                .join(" ")
                .to_lowercase()
                .chars()
                .take(500)
                .collect();
            let limits = Limits {
                description_chars: config.store_description_chars(),
                max_per_site: usize::try_from(config.store_max_per_site()).unwrap_or(5_000),
            };
            let kept = keep::keep(db, row.id, &jobs, &tags, full, now, &limits).await?;
            let quiet = if kept.new == 0 && kept.changed == 0 {
                quiet_before + 1
            } else {
                0
            };
            let status = if full { "ok" } else { "partial" };
            job_boards::record_fetch(db, row.id, now, Some(jobs.len()), None, None).await?;
            // What a site lists (its average pay, how well its jobs fit the
            // skills asked for) is kept by the store's read, since searches
            // no longer read the site.
            if !page
                && let Err(e) =
                    job_boards::keep_listed(db, row.id, &crate::readers::listed(&jobs), false, now)
                        .await
            {
                tracing::warn!(error = %e, "a site's listing could not be kept");
            }
            record(
                db,
                row.id,
                now,
                &Line {
                    status,
                    kept,
                    listed: kept.listed,
                    http_status: seen.status,
                    validators: &seen.validators,
                    error: None,
                    quiet,
                    duration_ms,
                    next_read_at: now + next_interval(base, quiet, page),
                },
            )
            .await?;
            Ok(Done { status, kept })
        }
        Err(FetchError::NotModified) => {
            keep::touch_all(db, row.id, now).await?;
            let listed = good
                .as_ref()
                .map_or(0, |good| u32::try_from(good.listed).unwrap_or(0));
            let quiet = quiet_before + 1;
            job_boards::record_fetch(
                db,
                row.id,
                now,
                Some(usize::try_from(listed).unwrap_or(0)),
                None,
                None,
            )
            .await?;
            record(
                db,
                row.id,
                now,
                &Line {
                    status: "not_modified",
                    kept: Kept::default(),
                    listed,
                    http_status: seen.status,
                    validators: &seen.validators,
                    error: None,
                    quiet,
                    duration_ms,
                    next_read_at: now + next_interval(base, quiet, page),
                },
            )
            .await?;
            Ok(Done {
                status: "not_modified",
                kept: Kept::default(),
            })
        }
        Err(e) => {
            let kind = e.kind();
            // The board and the error only: no URL, no answer.
            tracing::info!(board = %row.slug, error = %kind, "a site could not be read for the store");
            let backoff = match e {
                FetchError::RateLimited(secs) => {
                    let wait = secs.map_or(BACKOFF.0, Duration::from_secs).min(BACKOFF.1);
                    TimeDelta::from_std(wait).ok().map(|d| now + d)
                }
                FetchError::BackingOff => row.backoff_until,
                _ => None,
            };
            if !matches!(e, FetchError::BackingOff) {
                job_boards::record_fetch(db, row.id, now, None, Some(&kind), backoff).await?;
            }
            let wait = (base * 2).clamp(base, TimeDelta::days(1).max(base));
            let next = backoff.map_or(now + wait, |until| (now + wait).max(until));
            record(
                db,
                row.id,
                now,
                &Line {
                    status: "error",
                    kept: Kept::default(),
                    listed: 0,
                    http_status: seen.status,
                    validators: &seen.validators,
                    error: Some(&kind),
                    quiet: quiet_before,
                    duration_ms,
                    next_read_at: next,
                },
            )
            .await?;
            Ok(Done {
                status: "error",
                kept: Kept::default(),
            })
        }
    }
}

// ---- The owner's totals -------------------------------------------------------

/// How the job store is doing, for the owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StoreTotals {
    /// Whether the store is on (`APPLICAMP_SOURCES__STORE`).
    pub on: bool,
    /// Jobs it holds that their sites still list.
    pub jobs_stored: u32,
    /// Jobs it still holds that their sites no longer list.
    pub jobs_closed: u32,
    /// Jobs it stored for the first time in the last 7 days.
    pub new_7d: u32,
    /// Sites read today (UTC) without failing.
    pub sites_read_today: u32,
    /// Reads made today, failed ones included.
    pub reads_today: u32,
    /// Reads that failed today.
    pub failures_today: u32,
    /// Sites whose latest read failed.
    pub sites_failing: u32,
    /// When the latest read was.
    pub last_read_at: Option<DateTime<Utc>>,
}

fn count(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The totals of the job store at `now`.
pub async fn store_totals(db: &PgPool, on: bool, now: DateTime<Utc>) -> Result<StoreTotals> {
    let today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map_or(now, |at| at.and_utc());
    let (stored, closed, new): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE closed_at IS NULL),
                count(*) FILTER (WHERE closed_at IS NOT NULL),
                count(*) FILTER (WHERE first_seen_at >= $1)
           FROM sources.store_jobs",
    )
    .bind(now - TimeDelta::days(7))
    .fetch_one(db)
    .await?;
    let (sites, reads, failures): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(DISTINCT board_id) FILTER (WHERE status <> 'error'), count(*),
                count(*) FILTER (WHERE status = 'error')
           FROM sources.store_reads WHERE at >= $1",
    )
    .bind(today)
    .fetch_one(db)
    .await?;
    let failing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT DISTINCT ON (board_id) status FROM sources.store_reads
                                ORDER BY board_id, at DESC, id DESC) latest
          WHERE status = 'error'",
    )
    .fetch_one(db)
    .await?;
    let last_read_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT max(at) FROM sources.store_reads")
            .fetch_one(db)
            .await?;
    Ok(StoreTotals {
        on,
        jobs_stored: count(stored),
        jobs_closed: count(closed),
        new_7d: count(new),
        sites_read_today: count(sites),
        reads_today: count(reads),
        failures_today: count(failures),
        sites_failing: count(failing),
        last_read_at,
    })
}

/// How the job store is doing.
///
/// Jobs stored, new in the last 7 days, sites read today and the reads that
/// failed, for the board owners. No job is shown here.
#[utoipa::path(
    get,
    path = "/api/v1/boards/store",
    tag = "sources",
    operation_id = "sources.store_totals",
    security(("session" = [])),
    responses(
        (status = 200, description = "The totals.", body = StoreTotals),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 403, description = "`not_board_owner`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn totals(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<StoreTotals>> {
    owner_only(&deps, &state.db, principal.account_id).await?;
    Ok(Json(
        store_totals(&state.db, deps.config.store(), state.clock.now()).await?,
    ))
}
