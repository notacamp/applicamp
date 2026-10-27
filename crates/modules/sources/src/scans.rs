//! What was scanned (`sources.board_scans`): one row per board per search
//! (jobs read, passed the person's requirements, scored by the AI,
//! matched, the AI's cost, a skip or an error) and per discovery check.
//! Kept 90 days. A person sees their own searches' rows and every
//! discovery run's.

use std::collections::HashMap;

use applicamp_kernel::Result;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// How long scan rows are kept.
pub const KEEP: TimeDelta = TimeDelta::days(90);

/// What a run scanned on one board.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    pub board: Uuid,
    /// `read`, `skipped_poll_limit`, `error` or `checked`.
    pub outcome: &'static str,
    pub jobs_read: u32,
    pub passed_requirements: u32,
    pub scored_by_ai: u32,
    pub matched: u32,
    pub ai_cost_eur: f64,
    pub error_kind: Option<String>,
    pub duration_ms: u32,
    /// What a check found, in a few words.
    pub note: Option<String>,
}

/// Whose run it is.
#[derive(Debug, Clone, Copy)]
pub enum Run {
    /// `account`'s search.
    Search { account: Uuid, search: Uuid },
    /// A discovery run.
    Discovery(Uuid),
}

fn int(n: u32) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// Keep `scans` of `run`, at `at`.
pub async fn record(db: &PgPool, run: Run, scans: &[Scan], at: DateTime<Utc>) -> Result<()> {
    let (account, search, discovery) = match run {
        Run::Search { account, search } => (Some(account), Some(search), None),
        Run::Discovery(id) => (None, None, Some(id)),
    };
    for scan in scans {
        sqlx::query(
            "INSERT INTO sources.board_scans
                (id, account_id, search_id, discovery_run_id, board_id, at, outcome, jobs_read,
                 passed_requirements, scored_by_ai, matched, ai_cost_eur, error_kind, duration_ms,
                 note)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(Uuid::now_v7())
        .bind(account)
        .bind(search)
        .bind(discovery)
        .bind(scan.board)
        .bind(at)
        .bind(scan.outcome)
        .bind(int(scan.jobs_read))
        .bind(int(scan.passed_requirements))
        .bind(int(scan.scored_by_ai))
        .bind(int(scan.matched))
        .bind(scan.ai_cost_eur)
        .bind(&scan.error_kind)
        .bind(int(scan.duration_ms))
        .bind(
            scan.note
                .as_deref()
                .map(|n| n.chars().take(300).collect::<String>()),
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Forget scans older than [`KEEP`]. How many.
pub async fn prune(db: &PgPool, now: DateTime<Utc>) -> Result<u64> {
    Ok(sqlx::query("DELETE FROM sources.board_scans WHERE at < $1")
        .bind(now - KEEP)
        .execute(db)
        .await?
        .rows_affected())
}

/// A board's totals over some days: jobs read, matched, and the AI's cost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totals {
    pub read: u32,
    pub matched: u32,
    pub cost_eur: f64,
}

/// `account`'s totals per board over the last 7 and 30 days.
pub async fn totals(
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<HashMap<Uuid, (Totals, Totals)>> {
    let rows: Vec<(Uuid, i64, i64, f64, i64, i64, f64)> = sqlx::query_as(
        "SELECT board_id,
                coalesce(sum(jobs_read) FILTER (WHERE at >= $3), 0)::bigint,
                coalesce(sum(matched) FILTER (WHERE at >= $3), 0)::bigint,
                coalesce(sum(ai_cost_eur) FILTER (WHERE at >= $3), 0)::float8,
                coalesce(sum(jobs_read), 0)::bigint,
                coalesce(sum(matched), 0)::bigint,
                coalesce(sum(ai_cost_eur), 0)::float8
           FROM sources.board_scans
          WHERE account_id = $1 AND at >= $2
          GROUP BY board_id",
    )
    .bind(account)
    .bind(now - TimeDelta::days(30))
    .bind(now - TimeDelta::days(7))
    .fetch_all(db)
    .await?;
    let n = |v: i64| u32::try_from(v).unwrap_or(u32::MAX);
    Ok(rows
        .into_iter()
        .map(|(board, r7, m7, c7, r30, m30, c30)| {
            (
                board,
                (
                    Totals {
                        read: n(r7),
                        matched: n(m7),
                        cost_eur: c7,
                    },
                    Totals {
                        read: n(r30),
                        matched: n(m30),
                        cost_eur: c30,
                    },
                ),
            )
        })
        .collect())
}

/// A run, as the Scanned view lists it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    /// `search` or `discovery`.
    pub kind: String,
    pub id: Uuid,
    pub at: DateTime<Utc>,
    /// A search's `manual` or `nightly`; `weekly` for discovery.
    pub trigger: String,
    pub cost_usd: f64,
}

/// One board's line of a run.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScanRow {
    pub run_id: Uuid,
    pub board_id: Uuid,
    pub board_slug: String,
    pub board_name: String,
    pub outcome: String,
    pub jobs_read: i32,
    pub passed_requirements: i32,
    pub scored_by_ai: i32,
    pub matched: i32,
    pub ai_cost_eur: f64,
    pub error_kind: Option<String>,
    pub duration_ms: i32,
    pub note: Option<String>,
}

/// Up to `most` runs `account` may see that scanned `board` (any, if none),
/// only `search` if given, before `before`, newest first; with their rows.
pub async fn runs(
    db: &PgPool,
    account: Uuid,
    board: Option<Uuid>,
    search: Option<Uuid>,
    before: Option<DateTime<Utc>>,
    most: i64,
) -> Result<(Vec<RunRow>, Vec<ScanRow>)> {
    let runs: Vec<RunRow> = sqlx::query_as(
        "SELECT kind, id, at, trigger, cost_usd FROM (
            SELECT 'search' AS kind, s.id, s.started_at AS at, s.trigger, s.cost_usd
              FROM sources.searches s
             WHERE s.account_id = $1
               AND EXISTS (SELECT 1 FROM sources.board_scans b
                            WHERE b.search_id = s.id AND ($2::uuid IS NULL OR b.board_id = $2))
            UNION ALL
            SELECT 'discovery' AS kind, d.id, d.started_at AS at, 'weekly' AS trigger,
                   0::float8 AS cost_usd
              FROM sources.discovery_runs d
             WHERE EXISTS (SELECT 1 FROM sources.board_scans b
                            WHERE b.discovery_run_id = d.id
                              AND ($2::uuid IS NULL OR b.board_id = $2))
         ) AS runs
         WHERE ($3::uuid IS NULL OR id = $3) AND ($4::timestamptz IS NULL OR at < $4)
         ORDER BY at DESC, id DESC
         LIMIT $5",
    )
    .bind(account)
    .bind(board)
    .bind(search)
    .bind(before)
    .bind(most)
    .fetch_all(db)
    .await?;
    let ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let rows: Vec<ScanRow> = sqlx::query_as(
        "SELECT coalesce(b.search_id, b.discovery_run_id) AS run_id, b.board_id,
                j.slug AS board_slug, j.name AS board_name, b.outcome, b.jobs_read,
                b.passed_requirements, b.scored_by_ai, b.matched, b.ai_cost_eur, b.error_kind,
                b.duration_ms, b.note
           FROM sources.board_scans b
           JOIN sources.job_boards j ON j.id = b.board_id
          WHERE coalesce(b.search_id, b.discovery_run_id) = ANY($1)
            AND ($2::uuid IS NULL OR b.board_id = $2)
            AND (b.account_id = $3 OR b.account_id IS NULL)
          ORDER BY b.matched DESC, b.jobs_read DESC, j.name",
    )
    .bind(&ids)
    .bind(board)
    .bind(account)
    .fetch_all(db)
    .await?;
    Ok((runs, rows))
}
