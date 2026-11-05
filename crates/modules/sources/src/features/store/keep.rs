//! What a read leaves in the store: new jobs added, changed ones rewritten,
//! the ones seen again touched, the ones a full read no longer lists closed,
//! and the old ones deleted.

use std::collections::{HashMap, HashSet};

use applicamp_kernel::Result;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    dedupe,
    job::{Job, WorkMode},
    robots,
};

/// How long a job that no read has seen is kept at most, in days: a site
/// that stopped answering must not keep its jobs open for ever.
pub const STALE_DAYS: i64 = 30;
/// How long the log of reads is kept, besides the latest of each site.
pub const READS_KEPT: TimeDelta = TimeDelta::days(3);
/// A full read that would close more than this share of a site's open jobs
/// (when it has at least [`SUSPICIOUS_FROM`]) is taken for a partial answer:
/// nothing is closed.
const SUSPICIOUS_SHARE: usize = 2;
const SUSPICIOUS_FROM: i64 = 10;

/// The limits a read is kept within.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// A description is cut at this many characters.
    pub description_chars: usize,
    /// Jobs kept for one site, the newest.
    pub max_per_site: usize,
}

/// What keeping one read did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    /// Jobs the site listed.
    pub listed: u32,
    /// Stored for the first time.
    pub new: u32,
    /// Rewritten because the site changed them.
    pub changed: u32,
    /// Closed: a full read no longer lists them.
    pub closed: u32,
    /// Not stored or deleted: past their expiry, or over the site's limit.
    pub dropped: u32,
    /// A full read that listed far fewer jobs than the site had open: taken
    /// for a partial answer, nothing was closed.
    pub held_back: bool,
}

/// A job as stored.
#[derive(Debug, Clone)]
struct Row {
    source_ref: String,
    title: String,
    company: String,
    url: String,
    apply_url: Option<String>,
    company_url: Option<String>,
    location: Option<String>,
    regions: Vec<String>,
    work_mode: Option<&'static str>,
    salary_min: Option<f64>,
    salary_max: Option<f64>,
    salary_currency: Option<String>,
    salary_period: Option<String>,
    salary_text: Option<String>,
    posted_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    description: String,
    language: Option<String>,
    url_key: Option<String>,
    dedupe_key: Option<String>,
    content_hash: String,
}

fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

fn cut_some(text: Option<&str>, most: usize) -> Option<String> {
    text.map(|t| cut(t.trim(), most)).filter(|t| !t.is_empty())
}

/// `job` as the store keeps it; `None` for one with no title or no
/// reference.
fn row_of(job: &Job, limits: &Limits) -> Option<Row> {
    let title = cut(job.title.trim(), 300);
    let source_ref = cut(job.source_ref.trim(), 500);
    if title.is_empty() || source_ref.is_empty() {
        return None;
    }
    let board = job.board.as_ref();
    let pay = board.and_then(|b| b.pay.as_ref());
    let mut row = Row {
        source_ref,
        title,
        company: cut(job.company.trim(), 300),
        url: cut(job.url.trim(), 2_000),
        apply_url: cut_some(board.and_then(|b| b.apply_url.as_deref()), 2_000),
        company_url: cut_some(board.and_then(|b| b.company_url.as_deref()), 2_000),
        location: cut_some(job.location.as_deref(), 300),
        regions: board.map(|b| b.regions.clone()).unwrap_or_default(),
        work_mode: job.work_mode.map(WorkMode::as_str),
        salary_min: pay.and_then(|p| p.min),
        salary_max: pay.and_then(|p| p.max),
        salary_currency: cut_some(pay.and_then(|p| p.currency.as_deref()), 3),
        salary_period: cut_some(pay.and_then(|p| p.period.as_deref()), 20),
        salary_text: cut_some(job.salary_text.as_deref(), 300),
        posted_at: job.posted_at,
        expires_at: board.and_then(|b| b.expires_at),
        description: cut(&job.description, limits.description_chars),
        language: None,
        url_key: dedupe::job_url(job),
        dedupe_key: dedupe::dedupe_key(job),
        content_hash: String::new(),
    };
    row.content_hash = hash_of(&row);
    Some(row)
}

/// A fingerprint of everything a row says, to tell a changed job from one
/// seen again.
fn hash_of(row: &Row) -> String {
    let at = |t: Option<DateTime<Utc>>| t.map(|t| t.timestamp().to_string()).unwrap_or_default();
    let number = |n: Option<f64>| n.map(|n| format!("{n:.2}")).unwrap_or_default();
    let parts = [
        row.title.clone(),
        row.company.clone(),
        row.url.clone(),
        row.apply_url.clone().unwrap_or_default(),
        row.company_url.clone().unwrap_or_default(),
        row.location.clone().unwrap_or_default(),
        row.regions.join(","),
        row.work_mode.unwrap_or_default().to_owned(),
        number(row.salary_min),
        number(row.salary_max),
        row.salary_currency.clone().unwrap_or_default(),
        row.salary_period.clone().unwrap_or_default(),
        row.salary_text.clone().unwrap_or_default(),
        at(row.posted_at),
        at(row.expires_at),
        row.description.clone(),
    ];
    robots::fingerprint(&parts.join(" \u{241f} "))
}

/// Keep what a read of `board` listed: `jobs`. `full` says the read listed
/// everything the site has, so what it no longer lists is closed; `tags`
/// are the site's tags (lower-case words), kept with each job for the text
/// search.
pub async fn keep(
    db: &PgPool,
    board: Uuid,
    jobs: &[Job],
    tags: &str,
    full: bool,
    now: DateTime<Utc>,
    limits: &Limits,
) -> Result<Kept> {
    let mut kept = Kept::default();
    // Every job once, in the order the site lists it.
    let mut seen_refs: HashSet<String> = HashSet::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut expired: Vec<String> = Vec::new();
    for job in jobs {
        let Some(row) = row_of(job, limits) else {
            continue;
        };
        if !seen_refs.insert(row.source_ref.clone()) {
            continue;
        }
        // Past its expiry (a site whose terms drop expired jobs gives one):
        // never kept.
        if row.expires_at.is_some_and(|at| at <= now) {
            expired.push(row.source_ref.clone());
            continue;
        }
        rows.push(row);
    }
    kept.listed = u32::try_from(seen_refs.len()).unwrap_or(u32::MAX);
    kept.dropped += u32::try_from(expired.len()).unwrap_or(u32::MAX);

    // A site with more jobs than the limit keeps the newest.
    if rows.len() > limits.max_per_site {
        rows.sort_by_key(|row| std::cmp::Reverse(row.posted_at));
        let over = rows.len() - limits.max_per_site;
        rows.truncate(limits.max_per_site);
        kept.dropped += u32::try_from(over).unwrap_or(u32::MAX);
    }

    let mut tx = db.begin().await?;
    let known: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT source_ref, content_hash FROM sources.store_jobs WHERE board_id = $1",
    )
    .bind(board)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    let mut same: Vec<String> = Vec::new();
    for row in &rows {
        match known.get(&row.source_ref) {
            Some(hash) if *hash == row.content_hash => same.push(row.source_ref.clone()),
            Some(_) => {
                upsert(&mut tx, board, row, tags, now).await?;
                kept.changed += 1;
            }
            None => {
                upsert(&mut tx, board, row, tags, now).await?;
                kept.new += 1;
            }
        }
    }
    if !same.is_empty() {
        sqlx::query(
            "UPDATE sources.store_jobs SET last_seen_at = $2, closed_at = NULL
              WHERE board_id = $1 AND source_ref = ANY($3)",
        )
        .bind(board)
        .bind(now)
        .bind(&same)
        .execute(&mut *tx)
        .await?;
    }
    if !expired.is_empty() {
        sqlx::query("DELETE FROM sources.store_jobs WHERE board_id = $1 AND source_ref = ANY($2)")
            .bind(board)
            .bind(&expired)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE sources.store_jobs SET tags = $2 WHERE board_id = $1 AND tags <> $2")
        .bind(board)
        .bind(tags)
        .execute(&mut *tx)
        .await?;

    if full {
        let listed: Vec<String> = seen_refs.iter().cloned().collect();
        let (open, to_close): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE NOT (source_ref = ANY($2)))
               FROM sources.store_jobs WHERE board_id = $1 AND closed_at IS NULL",
        )
        .bind(board)
        .bind(&listed)
        .fetch_one(&mut *tx)
        .await?;
        let held = open >= SUSPICIOUS_FROM
            && usize::try_from(to_close).unwrap_or(usize::MAX) * SUSPICIOUS_SHARE
                > usize::try_from(open).unwrap_or(usize::MAX);
        if held {
            kept.held_back = true;
        } else if to_close > 0 {
            let closed = sqlx::query(
                "UPDATE sources.store_jobs SET closed_at = $2
                  WHERE board_id = $1 AND closed_at IS NULL AND NOT (source_ref = ANY($3))",
            )
            .bind(board)
            .bind(now)
            .bind(&listed)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            kept.closed = u32::try_from(closed).unwrap_or(u32::MAX);
        }
    }

    // What an earlier, larger listing left over the limit: the oldest go.
    let over = sqlx::query(
        "DELETE FROM sources.store_jobs
          WHERE id IN (SELECT id FROM sources.store_jobs WHERE board_id = $1
                        ORDER BY (closed_at IS NULL) DESC, coalesce(posted_at, first_seen_at) DESC
                       OFFSET $2)",
    )
    .bind(board)
    .bind(i64::try_from(limits.max_per_site).unwrap_or(i64::MAX))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    kept.dropped += u32::try_from(over).unwrap_or(u32::MAX);
    tx.commit().await?;
    Ok(kept)
}

/// Add `row`, or rewrite the job it is a new version of (reopening it).
async fn upsert(
    tx: &mut sqlx::PgConnection,
    board: Uuid,
    row: &Row,
    tags: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources.store_jobs
            (id, board_id, source_ref, title, company, url, apply_url, company_url, location,
             regions, work_mode, salary_min, salary_max, salary_currency, salary_period,
             salary_text, posted_at, expires_at, description, language, tags, url_key, dedupe_key,
             content_hash, first_seen_at, last_seen_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                 $19, $20, $21, $22, $23, $24, $25, $25)
         ON CONFLICT (board_id, source_ref) DO UPDATE
            SET title = excluded.title, company = excluded.company, url = excluded.url,
                apply_url = excluded.apply_url, company_url = excluded.company_url,
                location = excluded.location, regions = excluded.regions,
                work_mode = excluded.work_mode, salary_min = excluded.salary_min,
                salary_max = excluded.salary_max, salary_currency = excluded.salary_currency,
                salary_period = excluded.salary_period, salary_text = excluded.salary_text,
                posted_at = excluded.posted_at, expires_at = excluded.expires_at,
                description = excluded.description, language = excluded.language,
                tags = excluded.tags, url_key = excluded.url_key,
                dedupe_key = excluded.dedupe_key, content_hash = excluded.content_hash,
                last_seen_at = excluded.last_seen_at, closed_at = NULL",
    )
    .bind(Uuid::now_v7())
    .bind(board)
    .bind(&row.source_ref)
    .bind(&row.title)
    .bind(&row.company)
    .bind(&row.url)
    .bind(&row.apply_url)
    .bind(&row.company_url)
    .bind(&row.location)
    .bind(&row.regions)
    .bind(row.work_mode)
    .bind(row.salary_min)
    .bind(row.salary_max)
    .bind(&row.salary_currency)
    .bind(&row.salary_period)
    .bind(&row.salary_text)
    .bind(row.posted_at)
    .bind(row.expires_at)
    .bind(&row.description)
    .bind(&row.language)
    .bind(tags)
    .bind(&row.url_key)
    .bind(&row.dedupe_key)
    .bind(&row.content_hash)
    .bind(now)
    .execute(tx)
    .await?;
    Ok(())
}

/// A site said nothing changed (304): everything it listed is still there.
pub async fn touch_all(db: &PgPool, board: Uuid, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "UPDATE sources.store_jobs SET last_seen_at = $2 WHERE board_id = $1 AND closed_at IS NULL",
    )
    .bind(board)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// What a cleaning deleted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cleaned {
    /// Past their expiry.
    pub expired: u64,
    /// Closed for longer than the limit.
    pub closed: u64,
    /// Not seen by any read for [`STALE_DAYS`].
    pub stale: u64,
    /// Over the limit for all jobs.
    pub over: u64,
    /// Old lines of the log of reads.
    pub reads: u64,
}

/// Delete what the store should not keep: jobs past their expiry, closed
/// ones after `closed_for`, ones no read has seen for [`STALE_DAYS`], the
/// oldest above `max_total`, and the log of reads older than [`READS_KEPT`]
/// (each site's latest line stays: it is its schedule).
pub async fn clean(
    db: &PgPool,
    now: DateTime<Utc>,
    closed_for: TimeDelta,
    max_total: i64,
) -> Result<Cleaned> {
    let expired = sqlx::query(
        "DELETE FROM sources.store_jobs WHERE expires_at IS NOT NULL AND expires_at <= $1",
    )
    .bind(now)
    .execute(db)
    .await?
    .rows_affected();
    let closed = sqlx::query(
        "DELETE FROM sources.store_jobs WHERE closed_at IS NOT NULL AND closed_at < $1",
    )
    .bind(now - closed_for)
    .execute(db)
    .await?
    .rows_affected();
    let stale = sqlx::query("DELETE FROM sources.store_jobs WHERE last_seen_at < $1")
        .bind(now - TimeDelta::days(STALE_DAYS))
        .execute(db)
        .await?
        .rows_affected();
    let over = sqlx::query(
        "DELETE FROM sources.store_jobs
          WHERE id IN (SELECT id FROM sources.store_jobs
                        ORDER BY (closed_at IS NULL) DESC, coalesce(posted_at, first_seen_at) DESC
                       OFFSET $1)",
    )
    .bind(max_total)
    .execute(db)
    .await?
    .rows_affected();
    let reads = sqlx::query(
        "DELETE FROM sources.store_reads r
          WHERE r.at < $1
            AND EXISTS (SELECT 1 FROM sources.store_reads n
                         WHERE n.board_id = r.board_id AND (n.at, n.id) > (r.at, r.id))",
    )
    .bind(now - READS_KEPT)
    .execute(db)
    .await?
    .rows_affected();
    Ok(Cleaned {
        expired,
        closed,
        stale,
        over,
        reads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            description_chars: 20,
            max_per_site: 100,
        }
    }

    fn job(reference: &str, title: &str) -> Job {
        Job {
            source: crate::job::Source::Board,
            source_ref: reference.to_owned(),
            title: title.to_owned(),
            company: "Example Co".to_owned(),
            location: Some("Lisbon".to_owned()),
            work_mode: Some(WorkMode::Remote),
            board_remote: None,
            url: format!("https://example.com/jobs/{reference}"),
            posted_at: None,
            salary_text: None,
            description: "A long description that is cut at twenty characters".to_owned(),
            board: None,
        }
    }

    #[test]
    fn a_description_is_cut_and_a_job_without_a_title_is_not_kept() {
        let row = row_of(&job("1", "Engineer"), &limits()).unwrap();
        assert_eq!(row.description.chars().count(), 20);
        assert_eq!(row.work_mode, Some("remote"));
        assert!(row_of(&job("2", "  "), &limits()).is_none());
        assert!(row_of(&job("  ", "Engineer"), &limits()).is_none());
    }

    #[test]
    fn the_fingerprint_changes_with_what_the_job_says() {
        let a = row_of(&job("1", "Engineer"), &limits()).unwrap();
        let same = row_of(&job("1", "Engineer"), &limits()).unwrap();
        let other = row_of(&job("1", "Staff Engineer"), &limits()).unwrap();
        assert_eq!(a.content_hash, same.content_hash);
        assert_ne!(a.content_hash, other.content_hash);
        let moved = Job {
            location: Some("Porto".to_owned()),
            ..job("1", "Engineer")
        };
        assert_ne!(
            a.content_hash,
            row_of(&moved, &limits()).unwrap().content_hash
        );
    }
}
