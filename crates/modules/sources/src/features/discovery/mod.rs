//! Discovery: every week, look for job boards Applicamp does not know yet,
//! check each one automatically, and approve the ones whose check is clean;
//! the others wait in the owner's queue. No board's jobs are read into a
//! search before it is approved, by the app or the owner.
//!
//! - **Candidates**: the public lists in [`lists`] (names and links only,
//!   CC0 and MIT; never LinkedIn, Indeed or Glassdoor, ADR 0006), and boards
//!   someone suggested or the seed list holds that were never checked. At
//!   most `APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS` a run (200); each board
//!   records the list it was found through.
//! - **The check** (one request per path, robots respected): robots.txt of
//!   the feed's host (else the site's); the site (its language, its feeds,
//!   JSON-LD postings, a link to its terms); the terms page, whose
//!   sentences about scraping, robots, automation, aggregation, the API,
//!   linking back, attribution or commercial use are kept verbatim (at most
//!   10); then the config's own address (with the board's key, when it
//!   needs one), read with its own config for up to 20 sample jobs. A board
//!   with no usable config is probed instead: the site's feeds (RSS
//!   autodiscovery), `/api`, `/jobs/api`, `/feed`, `/rss`, then JSON-LD
//!   postings through `/sitemap.xml`; the first that reads gives the sample
//!   and a **proposed config** the owner may use (applied by itself only to
//!   a discovered or suggested board nobody edited). The sample's remote
//!   and worldwide shares set the priority; its salaries rank the candidate.
//! - **The terms, read by a model** ([`crate::terms`]): one small call per
//!   board (and again when its terms change), within the spending limits,
//!   gives a verdict, the sentence that decided it and tags for what the
//!   board offers; a board with no terms page is silent.
//! - **Approval by the app**: a candidate nobody edited whose config reads
//!   sample jobs, whose robots rules allow every path read, that is a job
//!   board (at least 5 sample items with a title and a company or apply
//!   link), that posted in the last 30 days, whose terms allow apps or say
//!   nothing, and that has no blocker and needs no key, is approved by the
//!   app (`approved_by = 'app'`). Anything else waits for the owner.
//! - **Seeded boards at start** (`sources.check_seeded_boards`): every seeded
//!   board waiting for the owner that was never checked with its current
//!   config is checked once, unless the research found its terms or robots
//!   rules forbid reading it.
//! - **Re-checks**: approved boards' robots rules and terms every 6 months;
//!   a change is flagged for the owner.
//!
//! `sources.discover_boards` runs weekly (Mondays, 04:00 UTC) when
//! `APPLICAMP_SOURCES__DISCOVERY` is on; `sources.check_board` checks one
//! board now (a suggestion, or the owner's "Check again").

#[cfg(test)]
mod tests;

use std::{collections::HashSet, sync::LazyLock, time::Instant};

use applicamp_kernel::{Error, JobContext, JobError, Result, jobs};
use chrono::{DateTime, Datelike as _, TimeDelta, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    SourcesDeps, board_keys,
    features::boards::{blockers, unique_slug},
    job::html_to_text,
    job_boards::{
        self, BoardConfig, BoardRow, Fields, Kind, Proposal, Report, SampleJob, Status,
        board_columns, host_of,
    },
    places,
    readers::{
        MAX_BOARD_BODY, MAX_PAGE, Reader, jsonld,
        mapping::{self, BoardRef},
        rss,
    },
    regions, robots,
    scans::{self, Scan},
    terms,
};

pub mod lists;

/// Sample jobs a check keeps.
pub const SAMPLE: usize = 20;
/// Terms sentences a check keeps, and the longest.
pub const SENTENCES: (usize, usize) = (10, 300);
/// How long until an approved board is checked again.
pub const RECHECK: TimeDelta = TimeDelta::days(182);

static LIST_ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:[-*]|\d+\.)\s*\[([^\]]{1,100})\]\((https?://[^)\s]{1,300})\)")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static HTML_LANG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<html\b[^>]*\blang\s*=\s*["']([a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,8})?)["']"#)
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\b[^>]*\bhref\s*=\s*["']([^"'#]{1,300})["'][^>]*>(.{0,200}?)</a>"#)
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static TERMS_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)terms|\btos\b|legal|conditions|nutzungsbedingungen|\bagb\b|termos|condiciones")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static TERMS_SENTENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)scrap|robot|automat|aggregat|\bapi\b|link(?:ing)? back|backlink|attribut|commercial|crawl|spider|harvest",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// A board the list names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub name: String,
    pub url: Url,
    /// The list's heading over it ("Remote", "Germany").
    pub section: String,
    /// Its feed, when the list gives one (read first by the check).
    pub feed: Option<Url>,
    /// Its country (ISO 3166-1), when the list gives one.
    pub country: Option<String>,
}

/// The boards a Markdown list names (`- [Name](https://…)`), under their
/// headings.
pub fn parse_list(markdown: &str) -> Vec<ListEntry> {
    let mut section = String::new();
    let mut entries = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            section = heading.trim().chars().take(60).collect();
            continue;
        }
        let Some(caps) = LIST_ENTRY.captures(line) else {
            continue;
        };
        let (Some(name), Some(url)) = (caps.get(1), caps.get(2)) else {
            continue;
        };
        if section.is_empty() || section.eq_ignore_ascii_case("contents") {
            continue;
        }
        if let Ok(url) = Url::parse(url.as_str()) {
            entries.push(ListEntry {
                name: name.as_str().trim().to_owned(),
                url,
                section: section.clone(),
                feed: None,
                country: None,
            });
        }
    }
    entries
}

/// Sites never added: their terms forbid automated reading (ADR 0006).
pub fn never(host: &str) -> bool {
    host == "linkedin.com"
        || host.ends_with(".linkedin.com")
        || host.starts_with("indeed.")
        || host.contains(".indeed.")
        || host.starts_with("glassdoor.")
        || host.contains(".glassdoor.")
        || host == "github.com"
}

/// The next Monday 04:00 UTC after `now`.
pub fn next_run(now: DateTime<Utc>) -> DateTime<Utc> {
    let days = (7 - i64::from(now.weekday().num_days_from_monday())) % 7;
    let mut at = (now + TimeDelta::days(days))
        .date_naive()
        .and_hms_opt(4, 0, 0)
        .map_or(now, |t| t.and_utc());
    if at <= now {
        at += TimeDelta::days(7);
    }
    at
}

/// The weekly discovery.
#[derive(Debug, Serialize, Deserialize)]
pub struct Discover {}

impl applicamp_kernel::Job for Discover {
    const NAME: &'static str = "sources.discover_boards";
    const QUEUE: &'static str = "boards";
    // Never retried: the next week tries again.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_next(&ctx.db, deps).await?;
        discover(deps, &ctx.db).await.map_err(JobError::from)
    }
}

/// Checks one board now.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckBoard {
    pub board: Uuid,
}

impl applicamp_kernel::Job for CheckBoard {
    const NAME: &'static str = "sources.check_board";
    const QUEUE: &'static str = "boards";
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        check_one(deps, &ctx.db, self.board)
            .await
            .map_err(JobError::from)
    }
}

/// Checks, once, the seeded boards waiting for the owner that were never
/// checked with their current config (at every start; idempotent).
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckSeeded {}

impl applicamp_kernel::Job for CheckSeeded {
    const NAME: &'static str = "sources.check_seeded_boards";
    const QUEUE: &'static str = "boards";
    // Never retried: the next start tries what is left.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        check_seeded(deps, &ctx.db)
            .await
            .map(|_| ())
            .map_err(JobError::from)
    }
}

/// Enqueue the seeded boards' check to run now, unless one is waiting.
/// Whether it was.
pub async fn schedule_seeded(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &CheckSeeded {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// A fingerprint of `row`'s config, as a check keeps it.
pub fn config_fingerprint(row: &BoardRow) -> String {
    robots::fingerprint(&row.config.0.to_string())
}

/// Whether the seeded board `row` is checked at start: nothing the research
/// found forbids reading it, and it was never checked, or checked with
/// another config.
pub fn seeded_check_due(row: &BoardRow) -> bool {
    if row.origin() != job_boards::Origin::Seed
        || row.status() != Status::Candidate
        || !row.blockers().is_empty()
    {
        return false;
    }
    match row.report() {
        None => true,
        Some(report) => {
            report.checked_at.is_none()
                || (!report.config_fingerprint.is_empty()
                    && report.config_fingerprint != config_fingerprint(row))
        }
    }
}

/// Check every seeded board [`seeded_check_due`] (at most
/// `APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS`), as one discovery run, until
/// an AI spending limit is reached. How many were checked.
pub async fn check_seeded(deps: &SourcesDeps, db: &PgPool) -> Result<usize> {
    let reader = reader(deps)?;
    let rows: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards
          WHERE origin = 'seed' AND status = 'candidate'
          ORDER BY created_at, slug"
    ))
    .fetch_all(db)
    .await?;
    let due: Vec<&BoardRow> = rows
        .iter()
        .filter(|row| seeded_check_due(row))
        .take(deps.config.discovery_max_checks())
        .collect();
    if due.is_empty() {
        return Ok(0);
    }
    let run = start_run(db, deps.clock.now()).await?;
    let mut checked = 0;
    let mut stopped = None;
    for row in &due {
        let outcome = check(deps, reader, db, row, run).await?;
        checked += 1;
        if outcome == Checked::BudgetReached {
            stopped = Some("ai_budget_reached");
            break;
        }
    }
    finish_run(db, run, (0, checked, 0), stopped, deps.clock.now()).await?;
    tracing::info!(checked, "seeded job boards checked");
    Ok(checked)
}

/// Make sure the next weekly discovery is scheduled, when it is on.
/// Whether one was added.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.discovery() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &Discover {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

/// From inside a discovery: schedule next week's (this one, running, does
/// not count).
async fn schedule_next(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.discovery() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_next_at(&mut tx, &Discover {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

async fn start_run(db: &PgPool, now: DateTime<Utc>) -> Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO sources.discovery_runs (id, started_at) VALUES ($1, $2)")
        .bind(id)
        .bind(now)
        .execute(db)
        .await?;
    Ok(id)
}

async fn finish_run(
    db: &PgPool,
    id: Uuid,
    counts: (usize, usize, usize),
    error: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let n = |v: usize| i32::try_from(v).unwrap_or(i32::MAX);
    sqlx::query(
        "UPDATE sources.discovery_runs
            SET finished_at = $2, found = $3, checked = $4, rechecked = $5, error_kind = $6
          WHERE id = $1",
    )
    .bind(id)
    .bind(now)
    .bind(n(counts.0))
    .bind(n(counts.1))
    .bind(n(counts.2))
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

fn reader(deps: &SourcesDeps) -> Result<&Reader> {
    deps.reader
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))
}

/// One discovery run: new candidates from the public lists, never-checked
/// candidates, re-checks due; then old scans forgotten. It stops, cleanly,
/// when an AI spending limit is reached: the rest waits for the next run.
pub async fn discover(deps: &SourcesDeps, db: &PgPool) -> Result<()> {
    let reader = reader(deps)?;
    let now = deps.clock.now();
    let run = start_run(db, now).await?;
    let max = deps.config.discovery_max_checks();
    let mut found = 0;
    let mut checked = 0;
    let mut error = None;
    let mut stopped = false;

    // New candidates from the lists.
    let mut listed: Vec<(ListEntry, String)> = Vec::new();
    let mut read_any = false;
    let mut asked_any = false;
    for source in &lists::SOURCES {
        if !deps.config.reads_list(source.id) {
            continue;
        }
        asked_any = true;
        let url = if source.id == lists::FIRST {
            deps.config.discovery_list_url.clone()
        } else {
            match Url::parse(source.url) {
                Ok(url) => url,
                Err(_) => continue,
            }
        };
        if let Ok(Some(body)) = reader.visit(db, &url).await {
            read_any = true;
            let through = source.found_through();
            listed.extend(
                lists::entries(source, &String::from_utf8_lossy(&body))
                    .into_iter()
                    .map(|entry| (entry, through.clone())),
            );
        }
    }
    if asked_any && !read_any {
        error = Some("list_unreachable");
    }
    let known: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT host FROM sources.job_boards")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();
    let mut seen = known;
    for (entry, through) in listed {
        if checked >= max || stopped {
            break;
        }
        let Some(host) = host_of(&entry.url) else {
            continue;
        };
        if never(&host) || !seen.insert(host.clone()) {
            continue;
        }
        let Some(id) = add_candidate(db, &entry, &host, &through, now).await? else {
            continue;
        };
        found += 1;
        if let Some(row) = job_boards::get(db, id).await? {
            checked += 1;
            if check(deps, reader, db, &row, run).await? == Checked::BudgetReached {
                stopped = true;
            }
        }
    }

    // Candidates never checked (suggested, or from the seed list).
    if !stopped {
        let waiting: Vec<BoardRow> = sqlx::query_as(concat!(
            "SELECT ",
            board_columns!(),
            " FROM sources.job_boards
              WHERE status = 'candidate' AND (report IS NULL OR report->>'checked_at' IS NULL)
                AND config ? 'endpoint' AND blockers = '[]'::jsonb
              ORDER BY created_at LIMIT $1"
        ))
        .bind(i64::try_from(max.saturating_sub(checked)).unwrap_or(0))
        .fetch_all(db)
        .await?;
        for row in &waiting {
            checked += 1;
            if check(deps, reader, db, row, run).await? == Checked::BudgetReached {
                stopped = true;
                break;
            }
        }
    }

    // Approved boards due a re-check.
    let due: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards
          WHERE status = 'approved' AND (recheck_after IS NULL OR recheck_after <= $1)
          ORDER BY recheck_after NULLS FIRST LIMIT $2"
    ))
    .bind(now)
    .bind(i64::try_from(max).unwrap_or(200))
    .fetch_all(db)
    .await?;
    for row in &due {
        if recheck(deps, reader, db, row, run).await? == Checked::BudgetReached {
            stopped = true;
        }
    }

    scans::prune(db, deps.clock.now()).await?;
    if stopped {
        error = Some("ai_budget_reached");
    }
    finish_run(
        db,
        run,
        (found, checked, due.len()),
        error,
        deps.clock.now(),
    )
    .await?;
    tracing::info!(
        found,
        checked,
        rechecked = due.len(),
        stopped,
        "job board discovery finished"
    );
    Ok(())
}

/// Check board `id` now, as its own discovery run.
pub async fn check_one(deps: &SourcesDeps, db: &PgPool, id: Uuid) -> Result<()> {
    let reader = reader(deps)?;
    let Some(row) = job_boards::get(db, id).await? else {
        return Ok(());
    };
    let now = deps.clock.now();
    let run = start_run(db, now).await?;
    let (counts, outcome) = if row.status() == Status::Approved {
        ((0, 0, 1), recheck(deps, reader, db, &row, run).await?)
    } else {
        ((0, 1, 0), check(deps, reader, db, &row, run).await?)
    };
    let stopped = (outcome == Checked::BudgetReached).then_some("ai_budget_reached");
    finish_run(db, run, counts, stopped, deps.clock.now()).await
}

/// Add the list's `entry` as a candidate; its id, or `None` when its site
/// is taken.
async fn add_candidate(
    db: &PgPool,
    entry: &ListEntry,
    host: &str,
    found_through: &str,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>> {
    let mut homepage = entry.url.clone();
    homepage.set_query(None);
    homepage.set_fragment(None);
    let country = places::countries_in(&entry.section);
    let country = entry
        .country
        .clone()
        .or_else(|| (country.len() == 1).then(|| country[0].to_owned()));
    // A feed the list names is where the check looks first.
    let (kind, config) = match &entry.feed {
        Some(feed) => (
            "rss",
            serde_json::json!({ "endpoint": feed.as_str(), "fields": {} }),
        ),
        None => ("json_api", serde_json::json!({})),
    };
    let priority = if entry.section.eq_ignore_ascii_case("remote") {
        "high"
    } else {
        "low"
    };
    let slug = unique_slug(db, host).await?;
    let id = Uuid::now_v7();
    let inserted = sqlx::query(
        "INSERT INTO sources.job_boards
            (id, slug, name, homepage, host, kind, priority, status, origin, country,
             config, found_through, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $9, $6, 'candidate', 'discovery', $7, $10, $11, $8, $8)
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(&slug)
    .bind(entry.name.chars().take(100).collect::<String>())
    .bind(homepage.as_str())
    .bind(host)
    .bind(priority)
    .bind(country)
    .bind(now)
    .bind(kind)
    .bind(sqlx::types::Json(&config))
    .bind(found_through.chars().take(300).collect::<String>())
    .execute(db)
    .await?;
    Ok((inserted.rows_affected() == 1).then_some(id))
}

/// How a check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checked {
    /// Done (the board is as checked as it can be now).
    Done,
    /// An AI spending limit was reached: the terms were not read, and the
    /// run stops asking the model.
    BudgetReached,
}

/// What the model read of a board's terms, kept until they change.
#[derive(Debug, Clone, Default)]
struct TermsState {
    verdict: Option<String>,
    quote: Option<String>,
    tags: Vec<String>,
    /// The fingerprint of the terms sentences the verdict is from.
    read_for: String,
    /// The board has no terms page: silent without a call.
    no_terms_page: bool,
    budget_reached: bool,
    cost_eur: f64,
}

impl TermsState {
    /// The verdict is of the terms as they are now.
    fn fresh(&self, key: &str) -> bool {
        self.verdict.is_some() && (self.no_terms_page || self.read_for == key)
    }

    /// The terms allow apps or say nothing, as they are now.
    fn allows(&self, key: &str) -> bool {
        self.fresh(key) && matches!(self.verdict.as_deref(), Some("allow" | "silent"))
    }
}

/// Read the terms of `row` with the model, unless its answer for these
/// sentences is kept, there is nothing to read, or the terms page could not
/// be reached (then the last answer stays, and is not fresh).
#[allow(clippy::too_many_arguments)]
async fn read_terms(
    deps: &SourcesDeps,
    db: &PgPool,
    row: &BoardRow,
    old: &Report,
    sentences: &[String],
    terms_read: bool,
    no_terms_page: bool,
    sample: &[(String, String, String)],
    now: DateTime<Utc>,
) -> TermsState {
    let key = robots::fingerprint(&sentences.join("\n"));
    let mut state = TermsState {
        verdict: row.terms_verdict.clone(),
        quote: row.terms_quote.clone(),
        tags: row.tags.clone(),
        read_for: old.terms_read_for.clone(),
        no_terms_page,
        ..TermsState::default()
    };
    if no_terms_page {
        // No terms page found: silent, with no call for the verdict.
        state.verdict = Some("silent".to_owned());
        state.quote = None;
    }
    let kept = state.read_for == key && row.terms_verdict.is_some();
    if kept || sample.is_empty() || !(terms_read || no_terms_page) {
        return state;
    }
    let Some(ai) = crate::features::search::configured(deps) else {
        return state;
    };
    let input = terms::Input {
        name: &row.name,
        has_terms: terms_read,
        sentences,
        sample,
    };
    match terms::read(ai, db, now, &input).await {
        Ok((reading, cost_usd)) => {
            state.verdict = Some(reading.verdict);
            state.quote = reading.quote;
            state.tags = reading.tags;
            state.read_for = key;
            state.cost_eur = cost_usd * ai.usd_to_eur();
        }
        Err(applicamp_ai::Error::OverBudget { .. } | applicamp_ai::Error::OverDailyCap { .. }) => {
            state.budget_reached = true
        }
        Err(e) => {
            tracing::warn!(board = %row.slug, error = %e, "a board's terms could not be read");
        }
    }
    state
}

static MAILTO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)mailto:([a-z0-9._%+-]{1,64}@[a-z0-9.-]{1,120}\.[a-z]{2,24})")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b([a-z0-9._%+-]{1,64}@[a-z0-9.-]{1,120}\.[a-z]{2,24})\b")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// An address to ask the board for permission: a `mailto:` link on its
/// terms page or homepage, else one written on its terms page.
pub fn contact_email(home: &str, terms: Option<&str>) -> Option<String> {
    let pick = |regex: &Regex, text: &str| {
        regex
            .captures(text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_ascii_lowercase())
    };
    terms
        .and_then(|t| pick(&MAILTO, t))
        .or_else(|| pick(&MAILTO, home))
        .or_else(|| terms.and_then(|t| pick(&EMAIL, &html_to_text(t))))
        .filter(|email| !email.ends_with(".png") && !email.ends_with(".jpg"))
}

/// How many sample items look like job postings: a title, and a company or
/// an apply link.
pub fn postings(sample: &[crate::job::Job]) -> usize {
    sample
        .iter()
        .filter(|job| {
            !job.title.trim().is_empty()
                && (!job.company.trim().is_empty()
                    || job.board.as_ref().is_some_and(|b| b.apply_url.is_some()))
        })
        .count()
}

/// The least sample items that must look like postings for a site to be a
/// job board (a CV tool or a blog has none).
pub const MIN_POSTINGS: usize = 5;

/// Whether the sample says the site is a job board that is alive: enough
/// postings, and one posted in the last 30 days.
pub fn is_a_live_job_board(sample: &[crate::job::Job], now: DateTime<Utc>) -> bool {
    postings(sample) >= MIN_POSTINGS
        && sample.iter().any(|job| {
            job.posted_at
                .is_some_and(|at| at >= now - TimeDelta::days(30))
        })
}

/// Pages one check reads, each path once.
struct Visits<'a> {
    reader: &'a Reader,
    db: &'a PgPool,
    seen: HashSet<String>,
}

impl Visits<'_> {
    /// `url`'s body, unless this check already asked for it, it is not
    /// found, or it failed.
    async fn get(&mut self, url: &Url) -> Option<String> {
        self.get_with(url, MAX_PAGE, &[])
            .await
            .map(|body| String::from_utf8_lossy(&body).into_owned())
    }

    /// `url`'s bytes (a feed decodes its own encoding), at most `max`, with
    /// a board's API key (`secret`) in the request's query only.
    async fn get_with(
        &mut self,
        url: &Url,
        max: usize,
        secret: &[(String, String)],
    ) -> Option<Vec<u8>> {
        let mut key = url.clone();
        key.set_fragment(None);
        if !self.seen.insert(key.to_string()) {
            return None;
        }
        match self.reader.visit_with(self.db, &key, max, secret).await {
            Ok(Some(body)) => Some(body),
            Ok(None) => None,
            Err(e) => {
                tracing::info!(error = %e, "a page of a board's check could not be read");
                None
            }
        }
    }
}

/// The robots.txt of `base`'s host.
async fn robots_of(visits: &mut Visits<'_>, base: &Url) -> Option<String> {
    let url = base.join("/robots.txt").ok()?;
    visits.get(&url).await
}

/// The link to the terms on `html`, on `home`'s site.
fn terms_link(html: &str, home: &Url) -> Option<Url> {
    let host = host_of(home)?;
    ANCHOR
        .captures_iter(html)
        .filter_map(|caps| {
            let href = caps.get(1)?.as_str();
            let text = caps.get(2).map_or("", |m| m.as_str());
            (TERMS_WORDS.is_match(href) || TERMS_WORDS.is_match(text))
                .then(|| home.join(href).ok())
                .flatten()
        })
        .find(|url| host_of(url).as_deref() == Some(host.as_str()))
}

/// The sentences of a terms page about automated reading and credit.
pub fn terms_sentences(html: &str) -> Vec<String> {
    let text = html_to_text(html);
    let mut sentences = Vec::new();
    for sentence in text.split_inclusive(['.', '!', '?', '\n']) {
        let sentence = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
        if sentence.len() > 20 && TERMS_SENTENCE.is_match(&sentence) {
            sentences.push(sentence.chars().take(SENTENCES.1).collect());
            if sentences.len() >= SENTENCES.0 {
                break;
            }
        }
    }
    sentences
}

/// The keys an item's field is often under.
const GUESSES: [(&str, &[&str]); 12] = [
    ("ref", &["id", "guid", "slug", "uuid"]),
    (
        "title",
        &["title", "jobTitle", "position", "job_title", "name"],
    ),
    (
        "company",
        &["companyName", "company_name", "company", "employer"],
    ),
    (
        "url",
        &["url", "link", "jobUrl", "job_url", "absolute_url", "guid"],
    ),
    (
        "apply_url",
        &[
            "applicationLink",
            "apply_url",
            "applyUrl",
            "application_url",
        ],
    ),
    (
        "location",
        &["location", "candidate_required_location", "jobGeo", "city"],
    ),
    ("regions", &["locationRestrictions", "regions", "countries"]),
    (
        "description",
        &["description", "jobDescription", "content", "body"],
    ),
    (
        "posted_at",
        &[
            "pubDate",
            "publication_date",
            "date",
            "created_at",
            "posted_at",
            "datePosted",
            "published_at",
        ],
    ),
    (
        "salary_min",
        &["minSalary", "salary_min", "salaryMin", "min_salary"],
    ),
    (
        "salary_max",
        &["maxSalary", "salary_max", "salaryMax", "max_salary"],
    ),
    (
        "salary_currency",
        &["currency", "salaryCurrency", "salary_currency"],
    ),
];

fn pointer_key(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

/// The array of jobs in an API's answer: where, whether its first item is
/// not a job, and the items.
fn job_items(answer: &Value) -> Option<(String, bool, &Vec<Value>)> {
    let looks_like_job = |item: &Value| {
        item.as_object().is_some_and(|o| {
            ["title", "jobTitle", "position", "name"]
                .iter()
                .any(|k| o.contains_key(*k))
        })
    };
    if let Some(items) = answer.as_array() {
        let skip = items.first().is_some_and(|f| !looks_like_job(f))
            && items.get(1).is_some_and(looks_like_job);
        return items
            .iter()
            .skip(usize::from(skip))
            .any(looks_like_job)
            .then(|| (String::new(), skip, items));
    }
    answer.as_object()?.iter().find_map(|(key, value)| {
        let items = value.as_array()?;
        items
            .first()
            .is_some_and(looks_like_job)
            .then(|| (pointer_key(key), false, items))
    })
}

/// A config reading `answer` (from `endpoint`), guessed from its keys.
pub fn propose_json(endpoint: &Url, answer: &Value) -> Option<BoardConfig> {
    let (items, skip_first, list) = job_items(answer)?;
    let first = list.get(usize::from(skip_first))?.as_object()?;
    let pick = |field: &str| {
        GUESSES
            .iter()
            .find(|(f, _)| *f == field)
            .and_then(|(_, keys)| {
                keys.iter().find(|k| {
                    first
                        .get(**k)
                        .is_some_and(|v| v.is_string() || v.is_number() || v.is_array())
                })
            })
            .map(|k| pointer_key(k))
    };
    let config = BoardConfig {
        endpoint: Some(endpoint.clone()),
        items: (!items.is_empty()).then_some(items),
        skip_first,
        poll_minutes: Some(60),
        fields: Fields {
            reference: pick("ref"),
            title: pick("title"),
            company: pick("company"),
            url: pick("url"),
            apply_url: pick("apply_url"),
            location: pick("location"),
            regions: pick("regions"),
            description: pick("description"),
            posted_at: pick("posted_at"),
            salary_min: pick("salary_min"),
            salary_max: pick("salary_max"),
            salary_currency: pick("salary_currency"),
            ..Fields::default()
        },
        ..BoardConfig::default()
    };
    config.validate(Kind::JsonApi).is_ok().then_some(config)
}

/// A config reading the feed at `endpoint`, whose first item is `item`.
pub fn propose_feed(endpoint: &Url, item: &rss::FeedItem) -> BoardConfig {
    let has = |name: &str| item.0.contains_key(name);
    let first = |names: &[&str]| names.iter().find(|n| has(n)).map(|n| (*n).to_owned());
    BoardConfig {
        endpoint: Some(endpoint.clone()),
        poll_minutes: Some(60),
        fields: Fields {
            reference: first(&["guid", "id", "link"]),
            title: Some("title".to_owned()),
            url: Some("link".to_owned()),
            company: first(&["company", "dc:creator", "author"]),
            regions: first(&["region", "location"]),
            description: first(&["description", "summary", "content", "content:encoded"]),
            posted_at: first(&["pubDate", "published", "updated", "dc:date"]),
            ..Fields::default()
        },
        ..BoardConfig::default()
    }
}

/// What a check found at one feed or API address.
struct Found {
    kind: Kind,
    url: Url,
    config: BoardConfig,
    sample: Vec<crate::job::Job>,
}

/// Read `body` (from `url`) as an API or a feed, with a proposed config and
/// its sample.
fn read_feed(url: &Url, body: &[u8], board: &BoardRef) -> Option<Found> {
    if let Ok(answer) = serde_json::from_slice::<Value>(body)
        && let Some(config) = propose_json(url, &answer)
    {
        let (_, skip, items) = job_items(&answer)?;
        let sample: Vec<_> = items
            .iter()
            .skip(usize::from(skip))
            .filter_map(|item| mapping::job_of(item, &config, board))
            .take(SAMPLE)
            .collect();
        return (!sample.is_empty()).then(|| Found {
            kind: Kind::JsonApi,
            url: url.clone(),
            config,
            sample,
        });
    }
    let items = rss::items(body);
    let config = propose_feed(url, items.first()?);
    let sample: Vec<_> = items
        .iter()
        .filter_map(|item| mapping::job_of(item, &config, board))
        .take(SAMPLE)
        .collect();
    (!sample.is_empty()).then(|| Found {
        kind: Kind::Rss,
        url: url.clone(),
        config,
        sample,
    })
}

/// Read `body` (from `url`) with the board's own `config`: its sample.
fn read_own(kind: Kind, config: &BoardConfig, url: &Url, body: &[u8], board: &BoardRef) -> Found {
    let sample: Vec<_> = match kind {
        Kind::JsonApi => serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|answer| {
                let items = match config.items.as_deref() {
                    Some(pointer) if !pointer.is_empty() => answer.pointer(pointer).cloned(),
                    _ => Some(answer),
                }?;
                let items = items.as_array()?.clone();
                Some(
                    items
                        .iter()
                        .skip(usize::from(config.skip_first))
                        .filter(|item| mapping::kept(item, config))
                        .filter_map(|item| mapping::job_of(item, config, board))
                        .take(SAMPLE)
                        .collect(),
                )
            })
            .unwrap_or_default(),
        Kind::Rss => rss::items(body)
            .iter()
            .filter_map(|item| mapping::job_of(item, config, board))
            .take(SAMPLE)
            .collect(),
        Kind::JsonldSitemap | Kind::Hn => Vec::new(),
    };
    Found {
        kind,
        url: url.clone(),
        config: config.clone(),
        sample,
    }
}

/// Posting pages a check reads through a sitemap, at most.
pub const SITEMAP_SAMPLE: usize = 3;

/// The part of a posting's path its sitemap's postings share: `/jobs/` for
/// `/jobs/123-engineer`.
fn job_path_of(url: &Url) -> String {
    let path = url.path().trim_start_matches('/');
    match path.split_once('/') {
        Some((first, rest)) if !first.is_empty() && !rest.is_empty() => format!("/{first}/"),
        _ => "/".to_owned(),
    }
}

/// JSON-LD postings through `sitemap` (its pages whose path names a job,
/// on `home`'s site and allowed): a few read for the sample, and a config.
async fn read_sitemap(
    visits: &mut Visits<'_>,
    sitemap: &Url,
    body: &[u8],
    job_path: Option<&str>,
    allowed: &(dyn Fn(&Url) -> bool + Sync),
    board: &BoardRef,
) -> Option<Found> {
    let text = String::from_utf8_lossy(body);
    let pages: Vec<Url> = jsonld::sitemap_urls(&text, job_path, 500)
        .into_iter()
        .filter(|url| host_of(url) == host_of(sitemap))
        .filter(|url| job_path.is_some() || url.path().to_ascii_lowercase().contains("job"))
        .filter(|url| allowed(url))
        .take(SITEMAP_SAMPLE)
        .collect();
    let first = pages.first()?.clone();
    let config = BoardConfig {
        sitemap: Some(sitemap.clone()),
        job_path: Some(job_path.map_or_else(|| job_path_of(&first), str::to_owned)),
        max_jobs: Some(20),
        poll_minutes: Some(360),
        ..BoardConfig::default()
    };
    let fields = jsonld::config_with_fields(&config);
    let mut sample = Vec::new();
    for page in &pages {
        let Some(html) = visits.get(page).await else {
            continue;
        };
        sample.extend(
            jsonld::postings(&html, page)
                .iter()
                .filter_map(|posting| mapping::job_of(posting, &fields, board)),
        );
    }
    (!sample.is_empty()).then(|| Found {
        kind: Kind::JsonldSitemap,
        url: sitemap.clone(),
        config,
        sample,
    })
}

fn share(part: usize, whole: usize) -> Option<f64> {
    // Twenty at most: exact as f64.
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// Check `row`: robots rules, its site, its terms, its config's address
/// (with its key) read with its own config, or, with no usable config,
/// common feed and API paths and a sitemap; keep the report (with a
/// proposed config when it has none that reads) and a line in `run`'s
/// scans.
#[allow(clippy::too_many_lines)]
pub async fn check(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    run: Uuid,
) -> Result<Checked> {
    let started = Instant::now();
    let now = deps.clock.now();
    let mut visits = Visits {
        reader,
        db,
        seen: HashSet::new(),
    };
    let Ok(home) = Url::parse(&row.homepage) else {
        return Ok(Checked::Done);
    };
    let config = row.config().unwrap_or_default();
    let kind = row.kind().unwrap_or(Kind::JsonApi);
    let usable = row.usable_config();
    let start = config.start(kind).cloned();
    let robots_base = start.clone().unwrap_or_else(|| home.clone());
    let secret = board_keys::query(reader.keys(), row, &config);

    // robots.txt of the feed's host (and of the site's, when another).
    let robots_text = robots_of(&mut visits, &robots_base).await;
    let mut robots = robots::report(robots_text.as_deref(), start.as_ref());
    robots.host = host_of(&robots_base);
    let home_rules = if host_of(&home) == robots.host {
        robots.rules.clone()
    } else {
        let text = robots_of(&mut visits, &home).await;
        robots::rules_for_us(text.as_deref().unwrap_or("")).0
    };
    let allowed = |url: &Url| {
        host_of(url) != host_of(&home) || robots::allowed(&home_rules, &robots::path_of(url)).0
    };

    // The site: its language, its feeds, its postings, its terms.
    let html = if allowed(&home) {
        visits.get(&home).await
    } else {
        None
    };
    let html = html.unwrap_or_default();
    let language = HTML_LANG
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_lowercase());
    let jsonld = jsonld::has_postings(&html);
    let terms_url = terms_link(&html, &home);
    let terms_html = match &terms_url {
        Some(url) if allowed(url) => visits.get(url).await,
        _ => None,
    };
    let terms_sentences = terms_html
        .as_deref()
        .map(terms_sentences)
        .unwrap_or_default();

    let contact = contact_email(&html, terms_html.as_deref());
    let board = BoardRef {
        id: row.id,
        slug: row.slug.clone(),
        name: row.name.clone(),
        drop_expired: false,
    };
    let mut found: Option<Found> = None;
    // Its own address, read with its own config when it reads (and with its
    // key, when it needs one: never asked without it).
    if let (Some(url), Ok(secret)) = (&start, &secret)
        && robots.endpoint_allowed != Some(false)
        && allowed(url)
        && let Some(body) = visits.get_with(url, MAX_BOARD_BODY, secret).await
    {
        found = match &usable {
            Some(own) if kind == Kind::JsonldSitemap => {
                let rules = robots.rules.clone();
                let allowed_page =
                    move |page: &Url| robots::allowed(&rules, &robots::path_of(page)).0;
                read_sitemap(
                    &mut visits,
                    url,
                    &body,
                    own.job_path.as_deref(),
                    &allowed_page,
                    &board,
                )
                .await
                .map(|f| Found {
                    config: own.clone(),
                    ..f
                })
            }
            Some(own) => Some(read_own(kind, own, url, &body, &board)),
            None => read_feed(url, &body, &board),
        };
    }
    // No usable config: the site's feeds, common feed and API paths, then
    // JSON-LD postings through its sitemap.
    if usable.is_none() && found.as_ref().is_none_or(|f| f.sample.is_empty()) {
        let mut probes: Vec<Url> = Vec::new();
        for link in rss::feed_links(&html).into_iter().take(2) {
            if let Ok(url) = home.join(&link)
                && host_of(&url) == host_of(&home)
            {
                probes.push(url);
            }
        }
        for path in PROBES {
            if let Ok(url) = home.join(path) {
                probes.push(url);
            }
        }
        for url in probes {
            if !allowed(&url) {
                continue;
            }
            let Some(body) = visits.get_with(&url, MAX_BOARD_BODY, &[]).await else {
                continue;
            };
            if let Some(read) = read_feed(&url, &body, &board) {
                found = Some(read);
                break;
            }
        }
        if found.is_none()
            && let Ok(sitemap) = home.join("/sitemap.xml")
            && allowed(&sitemap)
            && let Some(body) = visits.get_with(&sitemap, MAX_PAGE, &[]).await
        {
            found = read_sitemap(&mut visits, &sitemap, &body, None, &allowed, &board).await;
        }
    }

    // The sample's shares, salaries and the proposed priority.
    let sample: &[crate::job::Job] = found.as_ref().map_or(&[], |f| f.sample.as_slice());
    let remote = sample
        .iter()
        .filter(|job| {
            let regions = job
                .board
                .as_ref()
                .map(|b| b.regions.as_slice())
                .unwrap_or(&[]);
            let location = job.location.as_deref().unwrap_or("").to_lowercase();
            location.contains("remote")
                || regions::worldwide(regions)
                || job.title.to_lowercase().contains("remote")
        })
        .count();
    let worldwide = sample
        .iter()
        .filter(|job| {
            let regions = job
                .board
                .as_ref()
                .map(|b| b.regions.as_slice())
                .unwrap_or(&[]);
            regions::worldwide(regions)
                || job
                    .location
                    .as_deref()
                    .is_some_and(|l| regions::worldwide(&[l.to_owned()]))
        })
        .count();
    let remote_share = share(remote, sample.len());
    let worldwide_share = share(worldwide, sample.len());

    // The terms, read by the model (kept until they change).
    let old = row.report().unwrap_or_default();
    let key = robots::fingerprint(&terms_sentences.join("\n"));
    let sample_lines: Vec<(String, String, String)> = sample
        .iter()
        .map(|job| {
            (
                job.title.clone(),
                job.company.clone(),
                job.location.clone().unwrap_or_default(),
            )
        })
        .collect();
    let reading = read_terms(
        deps,
        db,
        row,
        &old,
        &terms_sentences,
        terms_html.is_some(),
        !html.is_empty() && terms_url.is_none(),
        &sample_lines,
        now,
    )
    .await;
    let mut flags = Vec::new();
    if reading.budget_reached {
        flags.push("the AI spending limit is reached: its terms are not read yet".to_owned());
    }
    if robots.endpoint_allowed == Some(false) {
        flags.push("robots.txt disallows the feed".to_owned());
    }
    if secret.is_err() {
        flags.push("its API needs a key: set one, then check again".to_owned());
    } else if found.is_some() && sample.is_empty() {
        flags.push("its address answered, but its config read no job from it".to_owned());
    } else if sample.is_empty() {
        flags.push("no feed or API found".to_owned());
    }
    if terms_sentences
        .iter()
        .any(|s| s.to_lowercase().contains("scrap") || s.to_lowercase().contains("automat"))
    {
        flags.push("its terms mention scraping or automated access: read them".to_owned());
    }
    if terms_url.is_none() {
        flags.push("no terms page found".to_owned());
    }
    // A proposed config and kind: applied by itself to a discovered or
    // suggested board nobody edited, offered to the owner for any other
    // board without a usable config.
    let auto_apply = !row.config_edited && row.origin() != job_boards::Origin::Seed;
    let proposed = found
        .as_ref()
        .filter(|f| !f.sample.is_empty() && (auto_apply || usable.is_none()))
        .map(|f| -> Result<Proposal> {
            Ok(Proposal {
                kind: f.kind,
                config: serde_json::to_value(&f.config).map_err(Error::internal)?,
            })
        })
        .transpose()?;
    let (new_kind, new_config, proposal) = match proposed {
        Some(p) if auto_apply => (p.kind.as_str().to_owned(), p.config, None),
        Some(p) => (row.kind.clone(), row.config.0.clone(), Some(p)),
        None => (row.kind.clone(), row.config.0.clone(), None),
    };
    let content_signal = robots
        .content_signal
        .as_ref()
        .and_then(|signal| serde_json::to_value(signal).ok());
    let report = Report {
        checked_at: Some(now),
        robots,
        terms_read_for: reading.read_for.clone(),
        terms_url: terms_url.as_ref().map(Url::to_string),
        terms_fingerprint: terms_html
            .as_deref()
            .map(robots::fingerprint)
            .unwrap_or_default(),
        terms_sentences,
        found_kind: found.as_ref().map(|f| f.kind),
        found_url: found.as_ref().map(|f| f.url.to_string()),
        jsonld,
        sample: sample
            .iter()
            .map(|job| SampleJob {
                title: job.title.clone(),
                company: job.company.clone(),
                location: job.location.clone(),
                url: job.url.clone(),
                regions: job
                    .board
                    .as_ref()
                    .map(|b| b.regions.clone())
                    .unwrap_or_default(),
            })
            .collect(),
        remote_share,
        worldwide_share,
        language,
        attribution: Some(row.name.clone()),
        flags,
        notes: old.notes,
        proposal,
        config_fingerprint: robots::fingerprint(&new_config.to_string()),
    };
    let high = remote_share.unwrap_or(0.0) >= 0.5 && worldwide_share.unwrap_or(0.0) >= 0.5;
    let priority = if row.origin() == job_boards::Origin::Seed {
        row.priority.clone()
    } else if high {
        "high".to_owned()
    } else {
        "low".to_owned()
    };
    sqlx::query(
        "UPDATE sources.job_boards
            SET report = $2, kind = $3, config = $4, priority = $5, updated_at = $6,
                terms_verdict = $7, terms_quote = $8, tags = $9, terms_url = $10,
                content_signal = $11, contact_email = coalesce($12, contact_email)
          WHERE id = $1",
    )
    .bind(row.id)
    .bind(sqlx::types::Json(
        serde_json::to_value(&report).map_err(Error::internal)?,
    ))
    .bind(&new_kind)
    .bind(sqlx::types::Json(&new_config))
    .bind(&priority)
    .bind(now)
    .bind(&reading.verdict)
    .bind(&reading.quote)
    .bind(&reading.tags)
    .bind(&report.terms_url)
    .bind(content_signal.as_ref().map(sqlx::types::Json))
    .bind(&contact)
    .execute(db)
    .await?;
    job_boards::keep_listed(db, row.id, &crate::readers::listed(sample), true, now).await?;
    let approved = reading_allows_and_approve(
        db,
        row,
        sample,
        reading.allows(&key),
        reading.budget_reached,
        now,
    )
    .await?;
    let note = match &found {
        Some(f) if !f.sample.is_empty() => {
            format!(
                "{} at {} · {} sample jobs · robots {}{}",
                match f.kind {
                    Kind::Rss => "Feed",
                    Kind::JsonldSitemap => "Sitemap",
                    Kind::JsonApi | Kind::Hn => "API",
                },
                f.url.path(),
                f.sample.len(),
                if report.robots.endpoint_allowed == Some(false) {
                    "disallow it"
                } else {
                    "allow it"
                },
                if report.proposal.is_some() {
                    " · config proposed"
                } else {
                    ""
                }
            ) + if approved {
                " · approved by the app"
            } else {
                ""
            }
        }
        _ if secret.is_err() => "Needs an API key".to_owned(),
        _ => "No feed or API found".to_owned(),
    };
    scans::record(
        db,
        scans::Run::Discovery(run),
        &[Scan {
            board: row.id,
            outcome: "checked",
            jobs_read: u32::try_from(sample.len()).unwrap_or(0),
            ai_cost_eur: reading.cost_eur,
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            note: Some(note),
            ..Scan::default()
        }],
        deps.clock.now(),
    )
    .await?;
    tracing::info!(board = %row.slug, found = found.is_some(), approved, "job board checked");
    Ok(if reading.budget_reached {
        Checked::BudgetReached
    } else {
        Checked::Done
    })
}

/// Approve `row` by the app when its check is clean, as stored just now:
/// still a candidate nobody edited, no blocker (the research's facts, what
/// its terms and robots rules say, a key it needs, a config that reads), a
/// config that reads its sample, terms that allow apps or say nothing as
/// they are now, a live job board. Whether it was.
async fn reading_allows_and_approve(
    db: &PgPool,
    row: &BoardRow,
    sample: &[crate::job::Job],
    terms_allow: bool,
    budget_reached: bool,
    now: DateTime<Utc>,
) -> Result<bool> {
    if budget_reached
        || !terms_allow
        || row.status() != Status::Candidate
        || row.config_edited
        || !is_a_live_job_board(sample, now)
    {
        return Ok(false);
    }
    let Some(fresh) = job_boards::get(db, row.id).await? else {
        return Ok(false);
    };
    if fresh.status() != Status::Candidate
        || fresh.config_edited
        || fresh.usable_config().is_none()
        || !blockers(&fresh).is_empty()
    {
        return Ok(false);
    }
    // Only if nobody (the owner) changed it in the meantime.
    let approved = sqlx::query(
        "UPDATE sources.job_boards
            SET status = 'approved', approved_by = 'app', approved_at = $2,
                recheck_after = $2 + interval '182 days', updated_at = $2
          WHERE id = $1 AND status = 'candidate' AND config_edited = false",
    )
    .bind(row.id)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        == 1;
    if approved {
        tracing::info!(board = %row.slug, "job board approved by the app: its check is clean");
    }
    Ok(approved)
}

/// Paths a check probes for a feed or an API when the board has no usable
/// config, after the feeds its site links to.
pub const PROBES: [&str; 4] = ["/api", "/jobs/api", "/feed", "/rss"];

/// Re-check an approved board: its robots rules and terms, flagging what
/// changed since the last check (never pausing it: the owner decides).
pub async fn recheck(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    run: Uuid,
) -> Result<Checked> {
    let started = Instant::now();
    let now = deps.clock.now();
    let mut visits = Visits {
        reader,
        db,
        seen: HashSet::new(),
    };
    let Ok(home) = Url::parse(&row.homepage) else {
        return Ok(Checked::Done);
    };
    let config = row.config().unwrap_or_default();
    let start = row.kind().and_then(|k| config.start(k)).cloned();
    let base = start.clone().unwrap_or_else(|| home.clone());
    let old = row.report().unwrap_or_default();
    let text = robots_of(&mut visits, &base).await;
    let mut robots = robots::report(text.as_deref(), start.as_ref());
    robots.host = host_of(&base);
    let mut flags = old.flags.clone();
    let mut flag = |text: &str| {
        if !flags.iter().any(|f| f == text) {
            flags.push(text.to_owned());
        }
    };
    if !old.robots.fingerprint.is_empty() && old.robots.fingerprint != robots.fingerprint {
        flag("robots.txt changed since the last check");
    }
    if robots.endpoint_allowed == Some(false) {
        flag("robots.txt now disallows the feed");
    }
    let mut terms_fingerprint = old.terms_fingerprint.clone();
    // What the model reads of its terms once they changed (once).
    let mut reread: Option<TermsState> = None;
    if let Some(url) = old.terms_url.as_deref().and_then(|u| Url::parse(u).ok())
        && let Some(html) = visits.get(&url).await
    {
        let fingerprint = robots::fingerprint(&html);
        if !old.terms_fingerprint.is_empty() && old.terms_fingerprint != fingerprint {
            flag("its terms page changed since the last check");
            let sample_lines: Vec<(String, String, String)> = old
                .sample
                .iter()
                .map(|job| {
                    (
                        job.title.clone(),
                        job.company.clone(),
                        job.location.clone().unwrap_or_default(),
                    )
                })
                .collect();
            let state = read_terms(
                deps,
                db,
                row,
                &old,
                &terms_sentences(&html),
                true,
                false,
                &sample_lines,
                now,
            )
            .await;
            if matches!(
                state.verdict.as_deref(),
                Some("forbid" | "ask_first" | "personal_only")
            ) {
                flag("its terms now forbid reading it or ask for permission first: read them");
            }
            reread = Some(state);
        }
        terms_fingerprint = fingerprint;
    }
    let content_signal = robots
        .content_signal
        .as_ref()
        .and_then(|signal| serde_json::to_value(signal).ok());
    let read_for = reread
        .as_ref()
        .filter(|state| state.verdict.is_some())
        .map_or_else(
            || old.terms_read_for.clone(),
            |state| state.read_for.clone(),
        );
    let report = Report {
        checked_at: Some(now),
        robots,
        terms_fingerprint,
        terms_read_for: read_for,
        flags,
        ..old
    };
    sqlx::query(
        "UPDATE sources.job_boards
            SET report = $2, recheck_after = $3, updated_at = $4,
                content_signal = coalesce($5, content_signal)
          WHERE id = $1",
    )
    .bind(row.id)
    .bind(sqlx::types::Json(
        serde_json::to_value(&report).map_err(Error::internal)?,
    ))
    .bind(now + RECHECK)
    .bind(now)
    .bind(content_signal.as_ref().map(sqlx::types::Json))
    .execute(db)
    .await?;
    // A board approved before: its status stays the owner's (or the app's
    // own approval, until the owner decides); only what it says is kept.
    if let Some(state) = reread.as_ref().filter(|state| state.verdict.is_some()) {
        sqlx::query(
            "UPDATE sources.job_boards SET terms_verdict = $2, terms_quote = $3, tags = $4
              WHERE id = $1",
        )
        .bind(row.id)
        .bind(&state.verdict)
        .bind(&state.quote)
        .bind(&state.tags)
        .execute(db)
        .await?;
    }
    let note = if report.robots.endpoint_allowed == Some(false) {
        "Re-checked: robots.txt now disallows the feed".to_owned()
    } else {
        "Re-checked: robots.txt allows the feed".to_owned()
    };
    scans::record(
        db,
        scans::Run::Discovery(run),
        &[Scan {
            board: row.id,
            outcome: "checked",
            ai_cost_eur: reread.as_ref().map_or(0.0, |state| state.cost_eur),
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            note: Some(note),
            ..Scan::default()
        }],
        deps.clock.now(),
    )
    .await?;
    Ok(if reread.is_some_and(|state| state.budget_reached) {
        Checked::BudgetReached
    } else {
        Checked::Done
    })
}
