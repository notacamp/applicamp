//! Company hiring pages (#87): the hiring systems companies publish their
//! jobs on (Greenhouse, Lever, Ashby, Workable, Recruitee, Teamtailor,
//! Personio) are checked once each, and every company page found through an
//! apply link is then approved by the app after one sample read.
//!
//! - **The system's check** ([`check_system`]): the robots.txt of the
//!   system's hosts (the reader's own, and where its job pages are), its
//!   terms read by the model like any board's ([`crate::terms`], one small
//!   call, kept until the terms change) and its `Content-Signal`. A system
//!   whose rules allow reading, whose terms allow apps or say nothing and
//!   that asks nothing of AI tools is approved by the app. A system whose
//!   content signal says `ai-train=no` waits for the owner as the blocker
//!   `needs_choice`: reading its pages is allowed, and it asks AI tools not
//!   to learn from them (Applicamp never trains on jobs, and the owner
//!   decides). Robots rules that forbid reading, and terms that forbid it or
//!   ask first, wait with their own blockers.
//! - **A company page's check** ([`check_page`]): no terms call, no model:
//!   one sample read through the system's reader. It is approved by the app
//!   when its system is approved, the read returns jobs and one was posted
//!   in the last 60 days. A page whose system waits or is declined stays
//!   waiting, or declined, with it.
//! - **The daily run** ([`run_all`], 05:00 UTC, and once at every start):
//!   checks the systems that are due, then at most
//!   `APPLICAMP_SOURCES__COMPANY_PAGE_CHECKS` new company pages (300), shared
//!   fairly between the approved systems, each host kept to the instance's
//!   request spacing. A rate limit stops that system for the day.
//! - **The weekly web search** ([`web_search`]): optional, off without a
//!   key, within a weekly and a monthly query cap.

#[cfg(test)]
mod tests;
pub mod web_search;

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    time::Instant,
};

use applicamp_kernel::{Error, JobContext, JobError, Result, jobs};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    SourcesConfig, SourcesDeps,
    features::{
        boards::{blocker, blockers},
        discovery::{self, Checked},
    },
    hiring_systems::{self, System},
    job::Source,
    job_boards::{
        self, Blocker, BoardRow, Kind, Report, RobotsRule, SampleJob, Status, board_columns,
    },
    readers::{Board, FetchError, Reader},
    robots,
    scans::{self, Scan},
    terms,
};

/// A company page is approved by the app only when a job was posted this
/// recently.
pub const RECENT: TimeDelta = TimeDelta::days(60);
/// How long a company page that is not approved yet is left alone.
pub const PAGE_RETRY: TimeDelta = TimeDelta::days(30);
/// How long one that failed to answer is left alone.
pub const PAGE_FAILED_RETRY: TimeDelta = TimeDelta::days(7);
/// How long a hiring system waiting for a decision is left alone.
pub const SYSTEM_RETRY: TimeDelta = TimeDelta::hours(20);

/// The next 05:00 UTC after `now`.
pub fn next_run(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut at = now
        .date_naive()
        .and_hms_opt(5, 0, 0)
        .map_or(now, |t| t.and_utc());
    if at <= now {
        at += TimeDelta::days(1);
    }
    at
}

/// The daily check of the hiring systems and the new company pages.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckPages {}

impl applicamp_kernel::Job for CheckPages {
    const NAME: &'static str = "sources.check_company_pages";
    const QUEUE: &'static str = "boards";
    // Never retried: tomorrow's run takes what is left.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_next(&ctx.db, deps).await?;
        run_all(deps, &ctx.db)
            .await
            .map(|_| ())
            .map_err(JobError::from)
    }
}

/// The same check, now: once at every start, and when a check of boards has
/// just registered company pages (idempotent: what was checked is not asked
/// again, and a day never checks more than the limit).
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckPagesSoon {}

impl applicamp_kernel::Job for CheckPagesSoon {
    const NAME: &'static str = "sources.check_company_pages_soon";
    const QUEUE: &'static str = "boards";
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        run_all(deps, &ctx.db)
            .await
            .map(|_| ())
            .map_err(JobError::from)
    }
}

/// Make sure the next daily check is scheduled, and one runs at start, when
/// company pages are on. Whether the daily one was added.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.company_pages() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added =
        jobs::enqueue_unique_at(&mut tx, &CheckPages {}, next_run(deps.clock.now())).await?;
    jobs::enqueue_unique_at(&mut tx, &CheckPagesSoon {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// Check the new company pages now (unless one check is waiting already), when
/// company pages are on. Whether one was added.
pub async fn schedule_soon(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.company_pages() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &CheckPagesSoon {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// From inside the daily run: schedule tomorrow's.
async fn schedule_next(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.company_pages() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_next_at(&mut tx, &CheckPages {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

fn reader(deps: &SourcesDeps) -> Result<&Reader> {
    deps.reader
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))
}

/// Queries the web search asked in `now`'s calendar month.
pub async fn queries_this_month(db: &PgPool, now: DateTime<Utc>) -> Result<u32> {
    let month = web_search::month_start(now);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sources.web_search_queries WHERE at >= $1")
            .bind(month)
            .fetch_one(db)
            .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// What a daily run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Hiring systems checked.
    pub systems: usize,
    /// Company pages checked.
    pub pages: usize,
    /// Of them, approved by the app.
    pub approved: usize,
}

/// One daily run: the hiring systems that are due, then the new company
/// pages of the approved ones, at most
/// `APPLICAMP_SOURCES__COMPANY_PAGE_CHECKS` of them.
pub async fn run_all(deps: &SourcesDeps, db: &PgPool) -> Result<Summary> {
    let reader = reader(deps)?;
    let now = deps.clock.now();
    crate::company_pages::ensure_systems(db, now).await?;
    let run = discovery::start_run(db, now).await?;
    let mut summary = Summary::default();
    let mut stopped = false;

    let systems: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards
          WHERE kind = 'hiring_system' AND status IN ('candidate', 'approved')
          ORDER BY system"
    ))
    .fetch_all(db)
    .await?;
    for row in &systems {
        if row.recheck_after.is_some_and(|at| at > now) {
            continue;
        }
        summary.systems += 1;
        if check_system(deps, reader, db, row, run, now).await? == Checked::BudgetReached {
            stopped = true;
        }
    }

    let approved: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards
          WHERE kind = 'hiring_system' AND status = 'approved'
          ORDER BY system"
    ))
    .fetch_all(db)
    .await?;
    // A day checks at most the limit, however many runs it has.
    let today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map_or(now, |t| t.and_utc());
    let checked_today: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sources.job_boards
          WHERE kind = 'company_page' AND (report->>'checked_at')::timestamptz >= $1",
    )
    .bind(today)
    .fetch_one(db)
    .await?;
    let mut remaining = deps
        .config
        .company_page_checks()
        .saturating_sub(usize::try_from(checked_today).unwrap_or(usize::MAX));
    // The new pages of each approved system, taken in turns so that none
    // takes the day's limit from the others.
    let mut queues: Vec<(Uuid, VecDeque<BoardRow>)> = Vec::new();
    for system in &approved {
        if remaining == 0 || stops_reading(system) {
            continue;
        }
        let pages: Vec<BoardRow> = sqlx::query_as(concat!(
            "SELECT ",
            board_columns!(),
            " FROM sources.job_boards
              WHERE kind = 'company_page' AND status = 'candidate' AND system_id = $1
                AND (recheck_after IS NULL OR recheck_after <= $2)
              ORDER BY created_at, slug LIMIT $3"
        ))
        .bind(system.id)
        .bind(now)
        .bind(i64::try_from(remaining).unwrap_or(i64::MAX))
        .fetch_all(db)
        .await?;
        if !pages.is_empty() {
            queues.push((system.id, pages.into()));
        }
    }
    let mut slowed: HashSet<Uuid> = HashSet::new();
    while remaining > 0 {
        let mut any = false;
        for (system, queue) in &mut queues {
            if remaining == 0 {
                break;
            }
            if slowed.contains(system) {
                continue;
            }
            let Some(page) = queue.pop_front() else {
                continue;
            };
            any = true;
            match check_page(reader, db, &page, now).await? {
                PageOutcome::Slowdown => {
                    slowed.insert(*system);
                }
                PageOutcome::Approved => {
                    summary.approved += 1;
                    summary.pages += 1;
                    remaining -= 1;
                }
                PageOutcome::Waiting | PageOutcome::Failed => {
                    summary.pages += 1;
                    remaining -= 1;
                }
            }
        }
        if !any {
            break;
        }
    }

    discovery::finish_run(
        db,
        run,
        (0, summary.systems + summary.pages, 0),
        stopped.then_some("ai_budget_reached"),
        deps.clock.now(),
    )
    .await?;
    tracing::info!(
        systems = summary.systems,
        pages = summary.pages,
        approved = summary.approved,
        stopped,
        "company pages checked"
    );
    Ok(summary)
}

/// A check of one row, now: a hiring system (its rules and terms) or a
/// company page (one sample read). The owner's "check again".
pub async fn check_row(deps: &SourcesDeps, db: &PgPool, row: &BoardRow) -> Result<()> {
    let reader = reader(deps)?;
    let now = deps.clock.now();
    match row.kind() {
        Some(Kind::HiringSystem) => {
            let run = discovery::start_run(db, now).await?;
            let outcome = check_system(deps, reader, db, row, run, now).await?;
            discovery::finish_run(
                db,
                run,
                (0, 1, 0),
                (outcome == Checked::BudgetReached).then_some("ai_budget_reached"),
                deps.clock.now(),
            )
            .await
        }
        Some(Kind::CompanyPage) => check_page(reader, db, row, now).await.map(|_| ()),
        _ => Ok(()),
    }
}

/// A system the daily run does not read company pages of: its robots rules
/// forbid it.
fn stops_reading(system: &BoardRow) -> bool {
    system
        .blockers()
        .iter()
        .any(|b| b.code == "robots_disallowed")
}

// ---- The hiring system's check ------------------------------------------

/// The address a system's reader asks at (a fake's, in tests).
fn reader_base(config: &SourcesConfig, source: Source, token: &str) -> Option<Url> {
    match source {
        Source::Greenhouse => Some(config.greenhouse_url.clone()),
        Source::Lever => Some(config.lever_url.clone()),
        Source::Ashby => Some(config.ashby_url.clone()),
        Source::Workable => Some(config.workable_url.clone()),
        Source::Recruitee => config.company_host(&config.recruitee_url, token),
        Source::Teamtailor => config.company_host(&config.teamtailor_url, token),
        Source::Personio => config.company_host(&config.personio_url, token),
        Source::Arbeitnow | Source::Hn | Source::Board => None,
    }
}

/// Where the check reads robots.txt, and the path it asks about there. The
/// system's own hosts, unless private hosts are allowed (a demo with
/// fakes): then the reader's address.
fn targets(config: &SourcesConfig, system: &System) -> Vec<(Url, String)> {
    let read = hiring_systems::read_path(system.source, system.probe);
    if config.allow_private_hosts() {
        return reader_base(config, system.source, system.probe)
            .map(|base| vec![(base, read)])
            .unwrap_or_default();
    }
    system
        .robots_hosts
        .iter()
        .enumerate()
        .filter_map(|(i, host)| {
            let base = Url::parse(&format!("https://{host}")).ok()?;
            let path = if i == 0 {
                read.clone()
            } else {
                format!("/{}", system.probe)
            };
            Some((base, path))
        })
        .collect()
}

async fn text(reader: &Reader, db: &PgPool, url: &Url) -> Option<String> {
    match reader.visit(db, url).await {
        Ok(Some(body)) => Some(String::from_utf8_lossy(&body).into_owned()),
        Ok(None) => None,
        Err(e) => {
            tracing::info!(error = %e, "a page of a hiring system's check could not be read");
            None
        }
    }
}

/// Whether robots.txt lets us read `url` (the rules of its host, asked once
/// each in `cache`).
async fn allowed_by_robots(
    reader: &Reader,
    db: &PgPool,
    url: &Url,
    cache: &mut HashMap<String, Vec<RobotsRule>>,
) -> bool {
    let Some(host) = job_boards::host_of(url) else {
        return false;
    };
    if !cache.contains_key(&host) {
        let rules = match url.join("/robots.txt") {
            Ok(robots_url) => text(reader, db, &robots_url)
                .await
                .map(|t| robots::rules_for_us(&t).0)
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        cache.insert(host.clone(), rules);
    }
    cache
        .get(&host)
        .is_none_or(|rules| robots::allowed(rules, &robots::path_of(url)).0)
}

/// What the model read of a system's terms.
#[derive(Debug, Clone, Default)]
struct Reading {
    verdict: Option<String>,
    quote: Option<String>,
    read_for: String,
    /// The system has no terms page: silent, with no call.
    no_terms_page: bool,
    budget_reached: bool,
    cost_eur: f64,
}

impl Reading {
    /// The verdict is of the terms as they are now.
    fn fresh(&self, key: &str) -> bool {
        self.verdict.is_some() && (self.no_terms_page || self.read_for == key)
    }

    /// The terms allow apps or say nothing, as they are now.
    fn allows(&self, key: &str) -> bool {
        self.fresh(key) && matches!(self.verdict.as_deref(), Some("allow" | "silent"))
    }
}

/// The terms of the system `row`, read once for all its companies: one small
/// call, unless its answer for these sentences is kept, there is nothing to
/// read, or the terms page could not be reached (the last answer stays, and
/// is not fresh).
#[allow(clippy::too_many_arguments)]
async fn read_terms(
    deps: &SourcesDeps,
    db: &PgPool,
    row: &BoardRow,
    old: &Report,
    sentences: &[String],
    terms_read: bool,
    no_terms_page: bool,
    now: DateTime<Utc>,
) -> Reading {
    let key = robots::fingerprint(&sentences.join("\n"));
    let mut state = Reading {
        verdict: row.terms_verdict.clone(),
        quote: row.terms_quote.clone(),
        read_for: old.terms_read_for.clone(),
        no_terms_page,
        ..Reading::default()
    };
    if no_terms_page {
        state.verdict = Some("silent".to_owned());
        state.quote = None;
        return state;
    }
    let kept = state.read_for == key && row.terms_verdict.is_some();
    if kept || !terms_read {
        return state;
    }
    if sentences.is_empty() {
        // A terms page with nothing about automated reading: silent, with no
        // call for the verdict.
        state.verdict = Some("silent".to_owned());
        state.quote = None;
        state.read_for = key;
        return state;
    }
    let Some(ai) = crate::features::search::configured(deps) else {
        return state;
    };
    let input = terms::Input {
        name: &row.name,
        has_terms: true,
        sentences,
        sample: &[],
    };
    match terms::read(ai, db, now, &input).await {
        Ok((reading, cost_usd)) => {
            state.verdict = Some(reading.verdict);
            state.quote = reading.quote;
            state.read_for = key;
            state.cost_eur = cost_usd * ai.usd_to_eur();
        }
        Err(applicamp_ai::Error::OverBudget { .. } | applicamp_ai::Error::OverDailyCap { .. }) => {
            state.budget_reached = true;
        }
        Err(e) => {
            tracing::warn!(system = %row.slug, error = %e, "a hiring system's terms could not be read");
        }
    }
    state
}

/// Check the hiring system `row`: robots rules and content signal of its
/// hosts, its terms read by the model, then approve it by the app when all
/// is clean. What it found is kept on the row; a system the owner paused or
/// declined is never touched.
#[allow(clippy::too_many_lines)]
pub async fn check_system(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    run: Uuid,
    now: DateTime<Utc>,
) -> Result<Checked> {
    let started = Instant::now();
    let Some(system) = row.system.as_deref().and_then(hiring_systems::named) else {
        return Ok(Checked::Done);
    };
    let old = row.report().unwrap_or_default();
    let mut cache: HashMap<String, Vec<RobotsRule>> = HashMap::new();

    // Robots rules and content signal, on every host the system reads.
    let mut report_robots: Option<crate::job_boards::RobotsReport> = None;
    let mut found: Vec<Blocker> = Vec::new();
    let mut signals: BTreeMap<String, String> = BTreeMap::new();
    let targets = targets(&deps.config, system);
    for (i, (base, path)) in targets.iter().enumerate() {
        let Ok(robots_url) = base.join("/robots.txt") else {
            continue;
        };
        let body = text(reader, db, &robots_url).await;
        let sample = base.join(path).ok();
        let mut report = robots::report(body.as_deref(), sample.as_ref());
        report.host = job_boards::host_of(base);
        if report.endpoint_allowed == Some(false) {
            found.push(blocker(
                "robots_disallowed",
                format!(
                    "robots.txt of {} forbids {path}{}",
                    report.host.clone().unwrap_or_default(),
                    report
                        .deciding_rule
                        .as_ref()
                        .map(|rule| format!(" ({rule})"))
                        .unwrap_or_default()
                ),
                Some(robots_url.to_string()),
            ));
        }
        for (name, value) in report.content_signal.iter().flatten() {
            let entry = signals.entry(name.clone()).or_insert_with(|| value.clone());
            if value == "no" {
                "no".clone_into(entry);
            }
        }
        if let Some(host) = report.host.clone() {
            cache.insert(host, report.rules.clone());
        }
        if i == 0 {
            report_robots = Some(report);
        }
    }
    let mut robots_report = report_robots.unwrap_or_default();
    let ai_train_no = signals.get("ai-train").map(String::as_str) == Some("no");
    robots_report.content_signal = (!signals.is_empty()).then(|| signals.clone());
    if ai_train_no {
        found.push(blocker(
            "needs_choice",
            format!(
                "Reading {}'s pages is allowed. It asks artificial intelligence tools not to \
                 learn from them. Applicamp never trains on jobs: approve it to read its \
                 company pages, or decline it.",
                system.name
            ),
            targets
                .first()
                .and_then(|(base, _)| base.join("/robots.txt").ok())
                .map(|u| u.to_string()),
        ));
    }

    // Its site and its terms, where robots rules allow.
    let home = if deps.config.allow_private_hosts() {
        targets.first().map(|(base, _)| base.clone())
    } else {
        Url::parse(system.homepage).ok()
    };
    let mut html = String::new();
    let mut terms_url: Option<Url> = None;
    let mut terms_html: Option<String> = None;
    if let Some(home) = &home {
        if allowed_by_robots(reader, db, home, &mut cache).await {
            html = text(reader, db, home).await.unwrap_or_default();
        }
        terms_url = discovery::terms_link(&html, home);
        if let Some(url) = &terms_url
            && allowed_by_robots(reader, db, url, &mut cache).await
        {
            terms_html = text(reader, db, url).await;
        }
    }
    let sentences = terms_html
        .as_deref()
        .map(discovery::terms_sentences)
        .unwrap_or_default();
    let key = robots::fingerprint(&sentences.join("\n"));
    let reading = read_terms(
        deps,
        db,
        row,
        &old,
        &sentences,
        terms_html.is_some(),
        !html.is_empty() && terms_url.is_none(),
        now,
    )
    .await;

    let mut flags = Vec::new();
    if reading.budget_reached {
        flags.push("the AI spending limit is reached: its terms are not read yet".to_owned());
    }
    if !reading.fresh(&key) && !reading.budget_reached {
        flags.push("its terms could not be read yet: it is checked again tomorrow".to_owned());
    }
    if !old.robots.fingerprint.is_empty()
        && old.robots.fingerprint != robots_report.fingerprint
        && row.status() == Status::Approved
    {
        flags.push("robots.txt changed since the last check".to_owned());
    }
    let notes = format!(
        "{} reading: robots rules {}; terms {}{}.",
        system.name,
        if found.iter().any(|b| b.code == "robots_disallowed") {
            "forbid it"
        } else {
            "allow it"
        },
        match reading.verdict.as_deref() {
            Some("allow") => "invite apps",
            Some("silent") => "say nothing about automated reading",
            Some("forbid") => "forbid automated reading",
            Some("ask_first") => "ask for permission first",
            Some("personal_only") => "allow personal use only",
            _ => "are not read yet",
        },
        if ai_train_no {
            "; it asks AI tools not to learn from its pages"
        } else {
            ""
        }
    );
    let report = Report {
        checked_at: Some(now),
        robots: robots_report,
        terms_url: terms_url.as_ref().map(Url::to_string),
        terms_fingerprint: terms_html
            .as_deref()
            .map(robots::fingerprint)
            .unwrap_or_default(),
        terms_read_for: reading.read_for.clone(),
        terms_sentences: sentences,
        attribution: Some(system.name.to_owned()),
        flags,
        notes,
        ..Report::default()
    };
    let content_signal = report
        .robots
        .content_signal
        .as_ref()
        .and_then(|signal| serde_json::to_value(signal).ok());
    let next = now
        + if row.status() == Status::Approved {
            discovery::RECHECK
        } else {
            SYSTEM_RETRY
        };
    sqlx::query(
        "UPDATE sources.job_boards
            SET report = $2, terms_verdict = $3, terms_quote = $4, terms_url = $5,
                content_signal = $6, blockers = $7, recheck_after = $8, updated_at = $9
          WHERE id = $1",
    )
    .bind(row.id)
    .bind(sqlx::types::Json(
        serde_json::to_value(&report).map_err(Error::internal)?,
    ))
    .bind(&reading.verdict)
    .bind(&reading.quote)
    .bind(&report.terms_url)
    .bind(content_signal.as_ref().map(sqlx::types::Json))
    .bind(sqlx::types::Json(
        serde_json::to_value(&found).map_err(Error::internal)?,
    ))
    .bind(next)
    .bind(now)
    .execute(db)
    .await?;

    let approved = approve_system(
        db,
        row,
        reading.allows(&key) && !reading.budget_reached,
        now,
    )
    .await?;
    scans::record(
        db,
        scans::Run::Discovery(run),
        &[Scan {
            board: row.id,
            outcome: "checked",
            ai_cost_eur: reading.cost_eur,
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            note: Some(format!(
                "{}{}",
                report.notes,
                if approved {
                    " Approved by the app."
                } else {
                    ""
                }
            )),
            ..Scan::default()
        }],
        deps.clock.now(),
    )
    .await?;
    tracing::info!(system = %row.slug, approved, "hiring system checked");
    Ok(if reading.budget_reached {
        Checked::BudgetReached
    } else {
        Checked::Done
    })
}

/// Approve the system `row` by the app when its check is clean as stored
/// just now: still a candidate, no blocker (rules, terms, a choice), terms
/// that allow apps or say nothing. Whether it was.
async fn approve_system(
    db: &PgPool,
    row: &BoardRow,
    terms_allow: bool,
    now: DateTime<Utc>,
) -> Result<bool> {
    if !terms_allow || row.status() != Status::Candidate {
        return Ok(false);
    }
    let Some(fresh) = job_boards::get(db, row.id).await? else {
        return Ok(false);
    };
    if fresh.status() != Status::Candidate || !blockers(&fresh).is_empty() {
        return Ok(false);
    }
    let approved = sqlx::query(
        "UPDATE sources.job_boards
            SET status = 'approved', approved_by = 'app', approved_at = $2,
                recheck_after = $2 + interval '182 days', updated_at = $2
          WHERE id = $1 AND status = 'candidate'",
    )
    .bind(row.id)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        == 1;
    if approved {
        tracing::info!(system = %row.slug, "hiring system approved by the app: its check is clean");
    }
    Ok(approved)
}

// ---- A company page's check ---------------------------------------------

/// How checking a company page went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOutcome {
    /// Approved by the app.
    Approved,
    /// Read, and kept waiting (no job, or none recent, or its system waits).
    Waiting,
    /// It did not answer.
    Failed,
    /// Its host asked us to slow down: no more of this system today.
    Slowdown,
}

/// Check the company page `page`: one sample read through its system's
/// reader, kept on the row; approved by the app when its system is approved,
/// the read returns jobs and one was posted in the last 60 days. No model,
/// no terms: the system's own check covers them.
pub async fn check_page(
    reader: &Reader,
    db: &PgPool,
    page: &BoardRow,
    now: DateTime<Utc>,
) -> Result<PageOutcome> {
    let (Some(source), Some(token)) = (
        page.system.as_deref().and_then(Source::parse),
        page.company_token.clone(),
    ) else {
        return Ok(PageOutcome::Failed);
    };
    let board = Board {
        kind: source,
        token,
        company: page.name.clone(),
        eu: page.eu,
    };
    let old = page.report().unwrap_or_default();
    match reader.board(db, &board).await {
        Ok(jobs) => {
            let newest = jobs.iter().filter_map(|job| job.posted_at).max();
            let recent = newest.is_some_and(|at| at >= now - RECENT);
            let name = jobs
                .iter()
                .map(|job| job.company.trim())
                .find(|company| !company.is_empty() && company.chars().count() <= 100)
                .map_or_else(|| page.name.clone(), str::to_owned);
            let mut flags = Vec::new();
            if jobs.is_empty() {
                flags.push("it lists no jobs".to_owned());
            } else if !recent {
                flags.push("no job was posted in the last 60 days".to_owned());
            }
            let report = Report {
                checked_at: Some(now),
                sample: jobs
                    .iter()
                    .take(5)
                    .map(|job| SampleJob {
                        title: job.title.clone(),
                        company: job.company.clone(),
                        location: job.location.clone(),
                        url: job.url.clone(),
                        regions: Vec::new(),
                    })
                    .collect(),
                attribution: Some(name.clone()),
                flags,
                notes: old.notes,
                ..Report::default()
            };
            sqlx::query(
                "UPDATE sources.job_boards
                    SET report = $2, name = $3, last_fetch_at = $4, last_fetch_jobs = $5,
                        last_error = NULL, recheck_after = $6, updated_at = $4
                  WHERE id = $1",
            )
            .bind(page.id)
            .bind(sqlx::types::Json(
                serde_json::to_value(&report).map_err(Error::internal)?,
            ))
            .bind(&name)
            .bind(now)
            .bind(i32::try_from(jobs.len()).unwrap_or(i32::MAX))
            .bind(now + PAGE_RETRY)
            .execute(db)
            .await?;
            if recent && approve_page(db, page, now).await? {
                return Ok(PageOutcome::Approved);
            }
            Ok(PageOutcome::Waiting)
        }
        Err(FetchError::RateLimited(_) | FetchError::BackingOff) => Ok(PageOutcome::Slowdown),
        Err(e) => {
            let kind = e.kind();
            let flag = match e {
                FetchError::Status(404) => "no company page answers there".to_owned(),
                _ => "its jobs could not be read".to_owned(),
            };
            tracing::info!(page = %page.slug, error = %kind, "a company page could not be read");
            let report = Report {
                checked_at: Some(now),
                flags: vec![flag],
                notes: old.notes,
                ..Report::default()
            };
            sqlx::query(
                "UPDATE sources.job_boards
                    SET report = $2, last_error = $3, recheck_after = $4, updated_at = $5
                  WHERE id = $1",
            )
            .bind(page.id)
            .bind(sqlx::types::Json(
                serde_json::to_value(&report).map_err(Error::internal)?,
            ))
            .bind(&kind)
            .bind(now + PAGE_FAILED_RETRY)
            .bind(now)
            .execute(db)
            .await?;
            Ok(PageOutcome::Failed)
        }
    }
}

/// Approve the company page `page` by the app, only while it still waits and
/// its system is approved. Whether it was.
async fn approve_page(db: &PgPool, page: &BoardRow, now: DateTime<Utc>) -> Result<bool> {
    let approved = sqlx::query(
        "UPDATE sources.job_boards p
            SET status = 'approved', approved_by = 'app', approved_at = $2,
                recheck_after = $2 + interval '60 days', updated_at = $2
          WHERE p.id = $1 AND p.status = 'candidate'
            AND EXISTS (SELECT 1 FROM sources.job_boards s
                         WHERE s.id = p.system_id AND s.status = 'approved')",
    )
    .bind(page.id)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        == 1;
    if approved {
        tracing::info!(page = %page.slug, "company page approved by the app");
    }
    Ok(approved)
}
