//! The Workable list (#88): every company Workable publishes jobs for, read
//! once a day, to grow the registry of company pages.
//!
//! Workable documents the list for programmatic use (help article "Utilizing
//! the XML Job Feed"): "The XML feed (https://www.workable.com/boards/workable.xml)
//! contains all job information for all our customers published on Jobs by
//! Workable. You will need to read/download the full file and then filter the
//! data on your end to get the jobs for one company only." and "The XML link
//! should be consumed programmatically; due to the size, it may not be
//! possible to load this via the browser." It carries two conditions: "This
//! is an advanced option and should not be used to connect with job boards
//! that have existing integrations with Workable", and the job links it gives
//! (`https://apply.workable.com/j/<code>`) are the way a candidate reaches
//! the job, so they are kept exactly as given. Its robots.txt (checked before
//! every download, for the path of the list) says `User-Agent: *`,
//! `Disallow:` for `/user_password_resets`, `/admin`, `/auth/google` and
//! `/j/` (that last one is on www.workable.com; the apply host's rules allow
//! all), and `Content-Signal: search=yes, ai-input=yes, ai-train=no`.
//!
//! - **The download** ([`download`]) streams: each chunk goes to
//!   [`feed::Scanner`], which hands over a job's company, short code, date
//!   and site and forgets the block, so memory holds one unfinished job and
//!   the companies of the last batch, never the body. It is sent
//!   `If-None-Match` / `If-Modified-Since` from the last complete one (a
//!   `304` ends it), is cut by a byte cap and a time limit (both configured)
//!   and keeps what it has processed when it is cut or interrupted: counts
//!   are written to `sources.workable_list` every few thousand jobs. A `429`
//!   or `503` is honoured (`Retry-After`, an hour to a day). No model, and
//!   the jobs themselves are not stored: only how many each company has.
//! - **The companies** keep their name, site, the short code of their newest
//!   job and counts. A company with a job posted in the last 60 days is
//!   eligible. The list's links name no company page (`/j/<code>`), so the
//!   page's token is found from one job's short link, which redirects to
//!   `apply.workable.com/<token>/j/<code>`: one request each, at most
//!   `APPLICAMP_SOURCES__WORKABLE_LIST_LOOKUPS` a day, robots.txt of the
//!   apply host respected, paced like any host. A link that already has the
//!   token needs no request.
//! - **The registry**: each eligible company with a token is upserted as a
//!   Workable `company_page` ("found through the Workable list", with when
//!   the list last had it and how many jobs), in batches. The app approves
//!   it when the Workable hiring system is approved ([`approve_listed`]): the
//!   list itself proves the company posts jobs, so the sample read is
//!   skipped, and the row's report says so. While the Workable system waits
//!   for the owner (its `ai-train=no`), its pages wait; a declined system
//!   declines its pages and the list is not read.
//! - **The exclusion list** (`APPLICAMP_SOURCES__WORKABLE_LIST_EXCLUDE`):
//!   names or sites of boards Workable already integrates with. A company on
//!   it is never added from the list. Workable publishes no such list, so it
//!   starts empty; the owner fills it from Workable's integrations.

pub mod feed;
#[cfg(test)]
mod tests;

use std::{collections::HashMap, time::Duration};

use applicamp_kernel::{Error, JobContext, JobError, Result, jobs};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    SourcesConfig, SourcesDeps, company_pages,
    features::company_pages::RECENT,
    hiring_systems,
    job::Source,
    net,
    readers::{self, BACKOFF, Reader, USER_AGENT, is_token},
    robots,
};

/// One download a day at most: a run within this long of the last is skipped.
pub const EVERY: TimeDelta = TimeDelta::hours(20);
/// Jobs after which the companies counted so far are written.
const FLUSH_JOBS: u64 = 20_000;
/// Companies written in one statement.
const BATCH: usize = 1_000;
/// A listed company page stays proven this long after the list last had it.
const FRESH_DAYS: i64 = 14;
/// How long a company whose address could not be found is left alone.
const LOOKUP_RETRY: TimeDelta = TimeDelta::days(7);

/// The next 06:00 UTC after `now`.
pub fn next_run(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut at = now
        .date_naive()
        .and_hms_opt(6, 0, 0)
        .map_or(now, |t| t.and_utc());
    if at <= now {
        at += TimeDelta::days(1);
    }
    at
}

/// The daily read of the list.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadList {}

impl applicamp_kernel::Job for ReadList {
    const NAME: &'static str = "sources.workable_list";
    const QUEUE: &'static str = "boards";
    // Never retried: tomorrow's run takes what is left.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: Duration = Duration::from_secs(2 * 60 * 60 + 30 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_next(&ctx.db, deps).await?;
        run(deps, &ctx.db).await.map(|_| ()).map_err(JobError::from)
    }
}

/// The same read, now: at start when the list was never read.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadListSoon {}

impl applicamp_kernel::Job for ReadListSoon {
    const NAME: &'static str = "sources.workable_list_soon";
    const QUEUE: &'static str = "boards";
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: Duration = Duration::from_secs(2 * 60 * 60 + 30 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        run(deps, &ctx.db).await.map(|_| ()).map_err(JobError::from)
    }
}

/// Make sure the next daily read is scheduled, and one runs now when the
/// list was never read, when the list is on. Whether the daily one was
/// added.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.workable_list() {
        return Ok(false);
    }
    let never: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM sources.workable_list_state
                             WHERE last_run_at IS NOT NULL)",
    )
    .fetch_one(db)
    .await?;
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &ReadList {}, next_run(deps.clock.now())).await?;
    if never {
        jobs::enqueue_unique_at(&mut tx, &ReadListSoon {}, deps.clock.now()).await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// From inside the daily run: schedule tomorrow's.
async fn schedule_next(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.workable_list() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_next_at(&mut tx, &ReadList {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

/// What a run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    /// How the download went: `complete`, `unchanged`, `interrupted`,
    /// `capped`, `refused`, `rate_limited`, `failed` or `skipped`.
    pub outcome: &'static str,
    /// Bytes read.
    pub bytes: u64,
    /// Jobs read.
    pub jobs: u64,
    /// Companies counted.
    pub companies: usize,
    /// Companies whose address on Workable was looked up.
    pub looked_up: usize,
    /// Company pages added to the registry.
    pub added: usize,
    /// Company pages approved by the app.
    pub approved: usize,
}

/// One run: the download (at most once a day), the lookups of company
/// addresses (at most the day's limit), the registry upsert, and the
/// approvals.
pub async fn run(deps: &SourcesDeps, db: &PgPool) -> Result<Summary> {
    let now = deps.clock.now();
    company_pages::ensure_systems(db, now).await?;
    let mut summary = Summary {
        outcome: "skipped",
        ..Summary::default()
    };
    let state: Option<State> = sqlx::query_as(
        "SELECT etag, last_modified, last_run_at, not_before
           FROM sources.workable_list_state WHERE id",
    )
    .fetch_optional(db)
    .await?;
    let state = state.unwrap_or_default();
    // The Workable system the pages belong to: a declined one is not read.
    let system: Option<String> = sqlx::query_scalar(
        "SELECT status FROM sources.job_boards WHERE kind = 'hiring_system' AND system = 'workable'",
    )
    .fetch_optional(db)
    .await?;
    if matches!(system.as_deref(), Some("declined" | "paused")) {
        tracing::info!(
            "the Workable list is not read: its hiring system is not approved or waiting"
        );
        return Ok(summary);
    }
    if state.last_run_at.is_some_and(|at| at > now - EVERY) {
        tracing::info!("the Workable list was read less than a day ago");
        return Ok(summary);
    }
    sqlx::query(
        "INSERT INTO sources.workable_list_state (id, last_run_at) VALUES (true, $1)
         ON CONFLICT (id) DO UPDATE SET last_run_at = excluded.last_run_at",
    )
    .bind(now)
    .execute(db)
    .await?;

    let run_id = Uuid::now_v7();
    let reader = deps
        .reader
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))?;
    let got = if state.not_before.is_some_and(|at| at > now) {
        tracing::info!("the Workable list asked us to wait");
        Download {
            not_before: state.not_before,
            ..Download::outcome("rate_limited")
        }
    } else {
        download(deps, reader, db, run_id, &state, now).await
    };
    summary.outcome = got.outcome;
    summary.bytes = got.bytes;
    summary.jobs = got.jobs;
    summary.companies = got.companies;

    summary.looked_up = look_up(deps, reader, db, now).await?;
    summary.added = promote(deps, db, now).await?;
    summary.approved = approve_listed(db, now).await?;

    sqlx::query(
        "UPDATE sources.workable_list_state
            SET etag = CASE WHEN $1 THEN $2 WHEN $8 THEN NULL ELSE etag END,
                last_modified = CASE WHEN $1 THEN $3 WHEN $8 THEN NULL ELSE last_modified END,
                not_before = $4, outcome = $5, bytes = $6, jobs = $7, companies = $9, added = $10
          WHERE id",
    )
    .bind(got.outcome == "complete")
    .bind(&got.etag)
    .bind(&got.last_modified)
    .bind(got.not_before)
    .bind(got.outcome)
    .bind(i64::try_from(got.bytes).unwrap_or(i64::MAX))
    .bind(i64::try_from(got.jobs).unwrap_or(i64::MAX))
    // A download cut short must not be taken for the last complete one.
    .bind(matches!(got.outcome, "interrupted" | "capped"))
    .bind(i32::try_from(got.companies).unwrap_or(i32::MAX))
    .bind(i32::try_from(summary.added).unwrap_or(i32::MAX))
    .execute(db)
    .await?;
    tracing::info!(
        outcome = got.outcome,
        bytes = summary.bytes,
        jobs = summary.jobs,
        companies = summary.companies,
        looked_up = summary.looked_up,
        added = summary.added,
        approved = summary.approved,
        "the Workable list read"
    );
    Ok(summary)
}

#[derive(Debug, Default, sqlx::FromRow)]
struct State {
    etag: Option<String>,
    last_modified: Option<String>,
    last_run_at: Option<DateTime<Utc>>,
    not_before: Option<DateTime<Utc>>,
}

/// How a download went.
#[derive(Debug, Default)]
struct Download {
    outcome: &'static str,
    bytes: u64,
    jobs: u64,
    companies: usize,
    etag: Option<String>,
    last_modified: Option<String>,
    not_before: Option<DateTime<Utc>>,
}

impl Download {
    fn outcome(outcome: &'static str) -> Self {
        Self {
            outcome,
            ..Self::default()
        }
    }
}

/// What the list says of one company, in this run.
#[derive(Debug, Default)]
struct Counts {
    name: String,
    website: Option<String>,
    sample_code: String,
    sample_at: Option<DateTime<Utc>>,
    token: Option<String>,
    jobs: i32,
    recent: i32,
    newest: Option<DateTime<Utc>>,
}

fn key_of(name: &str) -> String {
    name.trim().to_lowercase().chars().take(300).collect()
}

/// Count `item` for its company. `now` is the run's time (a job is recent
/// when posted in the last 60 days).
fn count(map: &mut HashMap<String, Counts>, item: feed::Item, now: DateTime<Utc>) {
    let entry = map.entry(key_of(&item.company)).or_default();
    if entry.name.is_empty() {
        entry.name = item.company.clone();
    }
    entry.jobs += 1;
    let recent = item
        .date
        .is_some_and(|at| at >= now - RECENT && at <= now + TimeDelta::days(1));
    if recent {
        entry.recent += 1;
    }
    if item.date > entry.newest {
        entry.newest = item.date;
    }
    if entry.website.is_none() {
        entry.website = item.website.clone();
    }
    if entry.sample_code.is_empty() || item.date > entry.sample_at {
        entry.sample_code = item.code.clone();
        entry.sample_at = item.date;
    }
    // A link of the form `<apply host>/<token>/j/<code>` already names the
    // company page.
    if entry.token.is_none()
        && let Some(link) = Url::parse(&item.url)
            .ok()
            .as_ref()
            .and_then(hiring_systems::company_link)
        && link.source == Source::Workable
    {
        entry.token = Some(link.token);
    }
}

/// Write the counts of `map` (one statement), and empty it.
async fn flush(
    db: &PgPool,
    run: Uuid,
    map: &mut HashMap<String, Counts>,
    now: DateTime<Utc>,
) -> Result<()> {
    let rows: Vec<(String, Counts)> = map
        .drain()
        .filter(|(key, c)| !key.is_empty() && !c.sample_code.is_empty())
        .collect();
    for chunk in rows.chunks(BATCH) {
        let keys: Vec<&str> = chunk.iter().map(|(k, _)| k.as_str()).collect();
        let names: Vec<String> = chunk
            .iter()
            .map(|(_, c)| c.name.chars().take(300).collect())
            .collect();
        let sites: Vec<Option<&str>> = chunk.iter().map(|(_, c)| c.website.as_deref()).collect();
        let codes: Vec<&str> = chunk.iter().map(|(_, c)| c.sample_code.as_str()).collect();
        let jobs: Vec<i32> = chunk.iter().map(|(_, c)| c.jobs).collect();
        let recent: Vec<i32> = chunk.iter().map(|(_, c)| c.recent).collect();
        let newest: Vec<Option<DateTime<Utc>>> = chunk.iter().map(|(_, c)| c.newest).collect();
        let tokens: Vec<Option<&str>> = chunk.iter().map(|(_, c)| c.token.as_deref()).collect();
        sqlx::query(
            "INSERT INTO sources.workable_list AS t
                (key, name, website, sample_code, jobs, recent_jobs, newest_job_at, last_seen_at,
                 run, token)
             SELECT c.key, c.name, c.website, c.code, c.jobs, c.recent, c.newest, $1, $2, c.token
               FROM unnest($3::text[], $4::text[], $5::text[], $6::text[], $7::int[], $8::int[],
                           $9::timestamptz[], $10::text[])
                    AS c(key, name, website, code, jobs, recent, newest, token)
             ON CONFLICT (key) DO UPDATE SET
                name = excluded.name,
                website = coalesce(excluded.website, t.website),
                sample_code = CASE
                    WHEN t.run <> excluded.run
                      OR excluded.newest_job_at >= coalesce(t.newest_job_at, '-infinity')
                    THEN excluded.sample_code ELSE t.sample_code END,
                jobs = CASE WHEN t.run = excluded.run THEN t.jobs + excluded.jobs
                            ELSE excluded.jobs END,
                recent_jobs = CASE WHEN t.run = excluded.run THEN t.recent_jobs + excluded.recent_jobs
                                   ELSE excluded.recent_jobs END,
                newest_job_at = CASE WHEN t.run = excluded.run
                                     THEN greatest(t.newest_job_at, excluded.newest_job_at)
                                     ELSE excluded.newest_job_at END,
                last_seen_at = excluded.last_seen_at,
                run = excluded.run,
                token = coalesce(excluded.token, t.token)",
        )
        .bind(now)
        .bind(run)
        .bind(&keys)
        .bind(&names)
        .bind(&sites)
        .bind(&codes)
        .bind(&jobs)
        .bind(&recent)
        .bind(&newest)
        .bind(&tokens)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// The path of robots.txt asks about, on the list's host.
async fn robots_allow(reader: &Reader, db: &PgPool, url: &Url) -> bool {
    let Ok(robots_url) = url.join("/robots.txt") else {
        return true;
    };
    match reader.visit(db, &robots_url).await {
        Ok(Some(body)) => {
            let rules = robots::rules_for_us(&String::from_utf8_lossy(&body)).0;
            robots::allowed(&rules, &robots::path_of(url)).0
        }
        // No robots.txt, or none that could be read: nothing forbids it.
        _ => true,
    }
}

fn client(config: &SourcesConfig, timeout: Option<Duration>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(readers::TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT);
    builder = match timeout {
        // The whole download, and no more than a minute of silence.
        Some(total) => builder.timeout(total).read_timeout(Duration::from_secs(60)),
        None => builder.timeout(readers::TIMEOUT),
    };
    if !config.allow_private_hosts() {
        builder = builder.dns_resolver(net::PublicResolver);
    }
    builder.build().map_err(Error::internal)
}

fn retry_after(response: &reqwest::Response, now: DateTime<Utc>) -> DateTime<Utc> {
    let secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(BACKOFF.0, Duration::from_secs)
        .clamp(BACKOFF.0, BACKOFF.1);
    now + TimeDelta::seconds(i64::try_from(secs.as_secs()).unwrap_or(3_600))
}

/// Stream the list and count its companies. Never fails: how it went is the
/// outcome, and what was counted is kept.
async fn download(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    run: Uuid,
    state: &State,
    now: DateTime<Utc>,
) -> Download {
    let config = &deps.config;
    let url = config.workable_list_url.clone();
    if net::check(&url, config.allow_private_hosts()).is_err() {
        return Download::outcome("refused");
    }
    if !robots_allow(reader, db, &url).await {
        tracing::info!("robots.txt does not allow the Workable list");
        return Download::outcome("refused");
    }
    let Ok(client) = client(config, Some(config.workable_list_timeout())) else {
        return Download::outcome("failed");
    };
    let mut request = client.get(url).header(
        reqwest::header::ACCEPT,
        "application/xml, text/xml;q=0.9, */*;q=0.1",
    );
    if let Some(etag) = &state.etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(since) = &state.last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, since);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!(error = %e.without_url(), "the Workable list could not be reached");
            return Download::outcome("failed");
        }
    };
    let status = response.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Download::outcome("unchanged");
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        return Download {
            not_before: Some(retry_after(&response, now)),
            ..Download::outcome("rate_limited")
        };
    }
    if !status.is_success() {
        tracing::warn!(status = status.as_u16(), "the Workable list did not answer");
        return Download::outcome("failed");
    }
    let header = |name: reqwest::header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.chars().take(200).collect::<String>())
    };
    let etag = header(reqwest::header::ETAG);
    let last_modified = header(reqwest::header::LAST_MODIFIED);

    let max = config.workable_list_max_bytes();
    let mut scanner = feed::Scanner::new();
    let mut map: HashMap<String, Counts> = HashMap::new();
    let mut bytes: u64 = 0;
    let mut written_at: u64 = 0;
    let mut outcome = "complete";
    let mut companies = 0usize;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let room = usize::try_from(max.saturating_sub(bytes)).unwrap_or(usize::MAX);
                let part = &chunk[..chunk.len().min(room)];
                bytes += part.len() as u64;
                scanner.push(part, |item| count(&mut map, item, now));
                if part.len() < chunk.len() {
                    outcome = "capped";
                    break;
                }
                if scanner.jobs - written_at >= FLUSH_JOBS {
                    written_at = scanner.jobs;
                    companies += map.len();
                    if let Err(e) = flush(db, run, &mut map, now).await {
                        tracing::warn!(error = %e, "the Workable list's counts could not be kept");
                        outcome = "failed";
                        break;
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e.without_url(), "the Workable list was interrupted");
                outcome = "interrupted";
                break;
            }
        }
    }
    // What was read is kept, however the download ended.
    companies += map.len();
    if outcome != "failed"
        && let Err(e) = flush(db, run, &mut map, now).await
    {
        tracing::warn!(error = %e, "the Workable list's counts could not be kept");
        outcome = "failed";
    }
    if outcome == "complete" {
        // Companies the list no longer names for three months are forgotten.
        let _ = sqlx::query("DELETE FROM sources.workable_list WHERE last_seen_at < $1")
            .bind(now - TimeDelta::days(90))
            .execute(db)
            .await;
    }
    let companies =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sources.workable_list WHERE run = $1")
            .bind(run)
            .fetch_one(db)
            .await
            .map_or(companies, |n| usize::try_from(n).unwrap_or(companies));
    Download {
        outcome,
        bytes,
        jobs: scanner.jobs,
        companies,
        etag,
        last_modified,
        not_before: None,
    }
}

/// The company token a job's short link redirects to
/// (`/<token>/j/<code>`).
fn token_of_redirect(base: &Url, location: &str) -> Option<String> {
    let target = base.join(location).ok()?;
    let token = target
        .path_segments()?
        .find(|segment| !segment.is_empty())?
        .to_owned();
    (token != "j" && is_token(&token)).then_some(token)
}

/// Find the Workable address of the companies the list names that have a
/// recent job and no token yet: one request each, the newest first, at most
/// the day's limit. How many were asked.
async fn look_up(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    now: DateTime<Utc>,
) -> Result<usize> {
    let config = &deps.config;
    let limit = config.workable_list_lookups();
    if limit == 0 {
        return Ok(0);
    }
    let waiting: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT key, name, website, sample_code FROM sources.workable_list
          WHERE token IS NULL AND recent_jobs > 0 AND (lookup_after IS NULL OR lookup_after <= $1)
          ORDER BY newest_job_at DESC NULLS LAST, key LIMIT $2",
    )
    .bind(now)
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(db)
    .await?;
    if waiting.is_empty() {
        return Ok(0);
    }
    let base = config.workable_url.clone();
    // The apply host's robots.txt is asked once, for the path of the links.
    let probe = base.join("/j/probe").map_err(Error::internal)?;
    if !robots_allow(reader, db, &probe).await {
        tracing::info!("robots.txt does not allow Workable's short links: no address is looked up");
        return Ok(0);
    }
    let http = client(config, None)?;
    let pause = if config.allow_private_hosts() {
        Duration::ZERO
    } else {
        config.request_spacing().max(Duration::from_secs(1))
    };
    let mut asked = 0;
    for (key, name, website, code) in waiting {
        if config.workable_list_excludes(&name, website.as_deref()) {
            set_lookup(db, &key, None, Some("excluded"), now + TimeDelta::days(30)).await?;
            continue;
        }
        if asked > 0 && !pause.is_zero() {
            tokio::time::sleep(pause).await;
        }
        asked += 1;
        let Ok(url) = base.join(&format!("/j/{code}")) else {
            continue;
        };
        if net::check(&url, config.allow_private_hosts()).is_err() {
            break;
        }
        match http.get(url.clone()).send().await {
            Ok(response) if response.status().is_redirection() => {
                let token = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|location| token_of_redirect(&url, location));
                match token {
                    Some(token) => set_lookup(db, &key, Some(&token), None, now).await?,
                    None => {
                        set_lookup(db, &key, None, Some("no_company"), now + LOOKUP_RETRY).await?;
                    }
                }
            }
            Ok(response)
                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE =>
            {
                tracing::info!("Workable asked us to slow down: no more addresses today");
                break;
            }
            Ok(response) => {
                let error = format!("http_{}", response.status().as_u16());
                set_lookup(db, &key, None, Some(&error), now + LOOKUP_RETRY).await?;
            }
            Err(_) => {
                set_lookup(
                    db,
                    &key,
                    None,
                    Some("unreachable"),
                    now + TimeDelta::days(1),
                )
                .await?;
            }
        }
    }
    Ok(asked)
}

async fn set_lookup(
    db: &PgPool,
    key: &str,
    token: Option<&str>,
    error: Option<&str>,
    after: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE sources.workable_list
            SET token = coalesce($2, token), lookup_error = $3, lookup_after = $4
          WHERE key = $1",
    )
    .bind(key)
    .bind(token)
    .bind(error)
    .bind(after)
    .execute(db)
    .await?;
    Ok(())
}

/// Upsert the companies with a recent job and a token as Workable company
/// pages, in batches. How many pages were new.
pub async fn promote(deps: &SourcesDeps, db: &PgPool, now: DateTime<Utc>) -> Result<usize> {
    #[derive(sqlx::FromRow)]
    struct Row {
        key: String,
        name: String,
        website: Option<String>,
        token: String,
        jobs: i32,
        last_seen_at: DateTime<Utc>,
        newest_job_at: Option<DateTime<Utc>>,
    }
    let mut added = 0;
    loop {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT key, name, website, token, jobs, last_seen_at, newest_job_at
               FROM sources.workable_list
              WHERE token IS NOT NULL AND recent_jobs > 0
                AND (promoted_at IS NULL OR last_seen_at > promoted_at)
              ORDER BY key LIMIT $1",
        )
        .bind(i64::try_from(BATCH).unwrap_or(1000))
        .fetch_all(db)
        .await?;
        if rows.is_empty() {
            break;
        }
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        // One page per token (two names may share one), none for a company on
        // the exclusion list.
        let mut seen: HashMap<String, &Row> = HashMap::new();
        for row in &rows {
            if !is_token(&row.token)
                || deps
                    .config
                    .workable_list_excludes(&row.name, row.website.as_deref())
                || deps.config.workable_list_excludes(&row.token, None)
            {
                continue;
            }
            let entry = seen.entry(row.token.to_ascii_lowercase()).or_insert(row);
            if row.jobs > entry.jobs {
                *entry = row;
            }
        }
        let mut pages: Vec<&Row> = seen.into_values().collect();
        pages.sort_by(|a, b| a.key.cmp(&b.key));
        if !pages.is_empty() {
            let ids: Vec<Uuid> = pages.iter().map(|_| Uuid::now_v7()).collect();
            let slugs: Vec<String> = pages
                .iter()
                .map(|r| company_pages::page_slug(Source::Workable, &r.token, false))
                .collect();
            let names: Vec<String> = pages
                .iter()
                .map(|r| r.name.chars().take(100).collect())
                .collect();
            let homepages: Vec<String> = pages
                .iter()
                .map(|r| hiring_systems::page_url(Source::Workable, &r.token, false))
                .collect();
            let tokens: Vec<&str> = pages.iter().map(|r| r.token.as_str()).collect();
            let jobs: Vec<i32> = pages.iter().map(|r| r.jobs).collect();
            let seen_at: Vec<DateTime<Utc>> = pages.iter().map(|r| r.last_seen_at).collect();
            let notes: Vec<String> = pages
                .iter()
                .map(|r| {
                    format!(
                        "Workable's list names this company with {} job{}{}. The app does not read the \
                         page first: the list itself shows the company posts jobs.",
                        r.jobs,
                        if r.jobs == 1 { "" } else { "s" },
                        r.newest_job_at
                            .map(|at| format!(", the newest posted {}", at.date_naive()))
                            .unwrap_or_default()
                    )
                })
                .collect();
            let inserted: Vec<bool> = sqlx::query_scalar(
                "INSERT INTO sources.job_boards
                    (id, slug, name, homepage, host, kind, priority, status, origin, config, terms,
                     system, company_token, eu, system_id, found_through, decline_reason, report,
                     list_seen_at, list_jobs, created_at, updated_at)
                 SELECT c.id, c.slug, c.name, c.homepage, s.host, 'company_page', 'low',
                        CASE WHEN s.status = 'declined' THEN 'declined' ELSE 'candidate' END,
                        'discovery', '{}'::jsonb, '{}'::jsonb, 'workable', c.token, false, s.id,
                        'the Workable list',
                        CASE WHEN s.status = 'declined' THEN 'Its hiring system was declined.' END,
                        jsonb_build_object('notes', c.notes, 'attribution', 'Workable'),
                        c.seen_at, c.jobs, $1, $1
                   FROM unnest($2::uuid[], $3::text[], $4::text[], $5::text[], $6::text[],
                               $7::int[], $8::timestamptz[], $9::text[])
                        AS c(id, slug, name, homepage, token, jobs, seen_at, notes)
                   JOIN sources.job_boards s ON s.kind = 'hiring_system' AND s.system = 'workable'
                 ON CONFLICT (system, lower(company_token), eu) WHERE kind = 'company_page'
                 DO UPDATE SET list_seen_at = excluded.list_seen_at,
                               list_jobs = excluded.list_jobs,
                               report = CASE WHEN job_boards.report->>'checked_at' IS NULL
                                             THEN coalesce(job_boards.report, '{}'::jsonb)
                                                  || jsonb_build_object('notes', excluded.report->'notes')
                                             ELSE job_boards.report END,
                               updated_at = excluded.updated_at
                 RETURNING (xmax = 0)",
            )
            .bind(now)
            .bind(&ids)
            .bind(&slugs)
            .bind(&names)
            .bind(&homepages)
            .bind(&tokens)
            .bind(&jobs)
            .bind(&seen_at)
            .bind(&notes)
            .fetch_all(db)
            .await?;
            added += inserted.iter().filter(|new| **new).count();
        }
        sqlx::query("UPDATE sources.workable_list SET promoted_at = $2 WHERE key = ANY($1)")
            .bind(&keys)
            .bind(now)
            .execute(db)
            .await?;
        if rows.len() < BATCH {
            break;
        }
    }
    Ok(added)
}

/// Approve, by the app, the listed Workable company pages still waiting
/// whose system is approved and which the list had in the last two weeks:
/// the list shows a job posted in the last 60 days, so there is no sample
/// read, and the row's report says so. How many.
pub async fn approve_listed(db: &PgPool, now: DateTime<Utc>) -> Result<usize> {
    let approved = sqlx::query(
        "UPDATE sources.job_boards p
            SET status = 'approved', approved_by = 'app', approved_at = $1,
                recheck_after = $1 + interval '60 days', updated_at = $1,
                report = coalesce(p.report, '{}'::jsonb) || jsonb_build_object(
                    'notes', format('Approved by the app: Workable''s list names this company with \
                                     %s job(s), one posted in the last 60 days. The list itself shows \
                                     the company posts jobs, so no sample of its page was read.',
                                    p.list_jobs))
          WHERE p.kind = 'company_page' AND p.status = 'candidate'
            AND p.list_seen_at >= $1 - ($2::float8 * interval '1 day')
            AND EXISTS (SELECT 1 FROM sources.job_boards s
                         WHERE s.id = p.system_id AND s.status = 'approved')",
    )
    .bind(now)
    .bind(FRESH_DAYS)
    .execute(db)
    .await?
    .rows_affected();
    Ok(usize::try_from(approved).unwrap_or(usize::MAX))
}
