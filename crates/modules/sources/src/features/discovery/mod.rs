//! Discovery: every week, look for job boards Applicamp does not know yet,
//! check each one automatically, and put it in the owner's queue. No model
//! is called, and no board's jobs are ever read into a search before the
//! owner approves it.
//!
//! - **Candidates**: the CC0 list `emredurukn/awesome-job-boards` (names
//!   and links only; never LinkedIn, Indeed or Glassdoor, ADR 0006), and
//!   boards someone suggested or the seed list holds that were never
//!   checked. At most [`MAX_CHECKS`] a run.
//! - **The check** (one request per path, robots respected): robots.txt of
//!   the feed's host (else the site's); the site (its language, its feeds,
//!   JSON-LD postings, a link to its terms); the terms page, whose
//!   sentences about scraping, robots, automation, aggregation, the API,
//!   linking back, attribution or commercial use are kept verbatim (at most
//!   10); then the feed or API (the config's, the site's feed, `/api`,
//!   `/jobs/api`), whose first answer gives up to 20 sample jobs through a
//!   proposed config. The sample's remote and worldwide shares set the
//!   priority; its salaries rank the candidate.
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
    SourcesDeps,
    features::boards::unique_slug,
    job::html_to_text,
    job_boards::{
        self, BoardConfig, BoardRow, Fields, Kind, Report, SampleJob, Status, board_columns,
        host_of,
    },
    places,
    readers::{
        Reader, jsonld,
        mapping::{self, BoardRef},
        rss,
    },
    regions, robots,
    scans::{self, Scan},
};

/// Boards checked per run, at most.
pub const MAX_CHECKS: usize = 20;
/// Sample jobs a check keeps.
pub const SAMPLE: usize = 20;
/// Terms sentences a check keeps, and the longest.
pub const SENTENCES: (usize, usize) = (10, 300);
/// How long until an approved board is checked again.
pub const RECHECK: TimeDelta = TimeDelta::days(182);

static LIST_ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*[-*]\s*\[([^\]]{1,100})\]\((https?://[^)\s]{1,300})\)")
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
        schedule(&ctx.db, deps).await?;
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

/// One discovery run: new candidates from the list, never-checked
/// candidates, re-checks due; then old scans forgotten.
pub async fn discover(deps: &SourcesDeps, db: &PgPool) -> Result<()> {
    let reader = reader(deps)?;
    let now = deps.clock.now();
    let run = start_run(db, now).await?;
    let mut found = 0;
    let mut checked = 0;
    let mut error = None;

    // New candidates from the list.
    let list = match reader.visit(db, &deps.config.discovery_list_url).await {
        Ok(Some(body)) => parse_list(&String::from_utf8_lossy(&body)),
        Ok(None) | Err(_) => {
            error = Some("list_unreachable");
            Vec::new()
        }
    };
    let known: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT host FROM sources.job_boards")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();
    let mut seen = known;
    for entry in list {
        if checked >= MAX_CHECKS {
            break;
        }
        let Some(host) = host_of(&entry.url) else {
            continue;
        };
        if never(&host) || !seen.insert(host.clone()) {
            continue;
        }
        let Some(id) = add_candidate(db, &entry, &host, now).await? else {
            continue;
        };
        found += 1;
        if let Some(row) = job_boards::get(db, id).await? {
            check(deps, reader, db, &row, run).await?;
            checked += 1;
        }
    }

    // Candidates never checked (suggested, or from the seed list).
    let waiting: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards
          WHERE status = 'candidate' AND (report IS NULL OR report->>'checked_at' IS NULL)
            AND config ? 'endpoint'
          ORDER BY created_at LIMIT $1"
    ))
    .bind(i64::try_from(MAX_CHECKS.saturating_sub(checked)).unwrap_or(0))
    .fetch_all(db)
    .await?;
    for row in &waiting {
        check(deps, reader, db, row, run).await?;
        checked += 1;
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
    .bind(i64::try_from(MAX_CHECKS).unwrap_or(20))
    .fetch_all(db)
    .await?;
    for row in &due {
        recheck(deps, reader, db, row, run).await?;
    }

    scans::prune(db, deps.clock.now()).await?;
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
    if row.status() == Status::Approved {
        recheck(deps, reader, db, &row, run).await?;
        finish_run(db, run, (0, 0, 1), None, deps.clock.now()).await
    } else {
        check(deps, reader, db, &row, run).await?;
        finish_run(db, run, (0, 1, 0), None, deps.clock.now()).await
    }
}

/// Add the list's `entry` as a candidate; its id, or `None` when its site
/// is taken.
async fn add_candidate(
    db: &PgPool,
    entry: &ListEntry,
    host: &str,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>> {
    let mut homepage = entry.url.clone();
    homepage.set_query(None);
    homepage.set_fragment(None);
    let country = places::countries_in(&entry.section);
    let country = (country.len() == 1).then(|| country[0].to_owned());
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
             created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'json_api', $6, 'candidate', 'discovery', $7, $8, $8)
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
    .execute(db)
    .await?;
    Ok((inserted.rows_affected() == 1).then_some(id))
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
        let mut key = url.clone();
        key.set_fragment(None);
        if !self.seen.insert(key.to_string()) {
            return None;
        }
        match self.reader.visit(self.db, &key).await {
            Ok(Some(body)) => Some(String::from_utf8_lossy(&body).into_owned()),
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

/// Read `url` as an API or a feed, with a proposed config and its sample.
fn read_feed(url: &Url, body: &str, board: &BoardRef) -> Option<Found> {
    if let Ok(answer) = serde_json::from_str::<Value>(body)
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
    let items = rss::items(body.as_bytes());
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

fn share(part: usize, whole: usize) -> Option<f64> {
    // Twenty at most: exact as f64.
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// Check `row`: robots rules, its site, its terms, a feed or an API and a
/// sample; keep the report (and a proposed config, unless the owner wrote
/// one) and a line in `run`'s scans.
pub async fn check(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    run: Uuid,
) -> Result<()> {
    let started = Instant::now();
    let now = deps.clock.now();
    let mut visits = Visits {
        reader,
        db,
        seen: HashSet::new(),
    };
    let Ok(home) = Url::parse(&row.homepage) else {
        return Ok(());
    };
    let config = row.config().unwrap_or_default();
    let kind = row.kind().unwrap_or(Kind::JsonApi);
    let start = config.start(kind).cloned();
    let robots_base = start.clone().unwrap_or_else(|| home.clone());

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
    let allowed = |url: &Url| robots::allowed(&home_rules, &robots::path_of(url)).0;

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

    // A feed or an API, and a sample through a proposed config.
    let board = BoardRef {
        id: row.id,
        slug: row.slug.clone(),
        name: row.name.clone(),
        drop_expired: false,
    };
    let mut candidates: Vec<Url> = start.iter().cloned().collect();
    for link in rss::feed_links(&html).into_iter().take(2) {
        if let Ok(url) = home.join(&link)
            && host_of(&url) == host_of(&home)
        {
            candidates.push(url);
        }
    }
    for path in ["/api", "/jobs/api"] {
        if let Ok(url) = home.join(path) {
            candidates.push(url);
        }
    }
    let mut found: Option<Found> = None;
    for url in candidates {
        let same_rules = host_of(&url) == host_of(&home);
        if same_rules && !allowed(&url) {
            continue;
        }
        let Some(body) = visits.get(&url).await else {
            continue;
        };
        if let Some(read) = read_feed(&url, &body, &board) {
            found = Some(read);
            break;
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
    let mut flags = Vec::new();
    if robots.endpoint_allowed == Some(false) {
        flags.push("robots.txt disallows the feed".to_owned());
    }
    if found.is_none() {
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
    let old = row.report().unwrap_or_default();
    let report = Report {
        checked_at: Some(now),
        robots,
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
    };
    let high = remote_share.unwrap_or(0.0) >= 0.5 && worldwide_share.unwrap_or(0.0) >= 0.5;
    // A proposed config and kind, unless the owner wrote one or the seed
    // list did.
    let propose = !row.config_edited && row.origin() != job_boards::Origin::Seed;
    let (new_kind, new_config) = match (&found, propose) {
        (Some(f), true) => (
            f.kind.as_str().to_owned(),
            serde_json::to_value(&f.config).map_err(Error::internal)?,
        ),
        _ => (row.kind.clone(), row.config.0.clone()),
    };
    let priority = if row.origin() == job_boards::Origin::Seed {
        row.priority.clone()
    } else if high {
        "high".to_owned()
    } else {
        "low".to_owned()
    };
    sqlx::query(
        "UPDATE sources.job_boards
            SET report = $2, kind = $3, config = $4, priority = $5, updated_at = $6
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
    .execute(db)
    .await?;
    job_boards::keep_listed(db, row.id, &crate::readers::listed(sample), true, now).await?;
    let note = match &found {
        Some(f) => format!(
            "{} at {} · {} sample jobs · robots {}",
            if f.kind == Kind::Rss { "Feed" } else { "API" },
            f.url.path(),
            f.sample.len(),
            if report.robots.endpoint_allowed == Some(false) {
                "disallow it"
            } else {
                "allow it"
            }
        ),
        None => "No feed or API found".to_owned(),
    };
    scans::record(
        db,
        scans::Run::Discovery(run),
        &[Scan {
            board: row.id,
            outcome: "checked",
            jobs_read: u32::try_from(sample.len()).unwrap_or(0),
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            note: Some(note),
            ..Scan::default()
        }],
        deps.clock.now(),
    )
    .await?;
    tracing::info!(board = %row.slug, found = found.is_some(), "job board checked");
    Ok(())
}

/// Re-check an approved board: its robots rules and terms, flagging what
/// changed since the last check (never pausing it: the owner decides).
pub async fn recheck(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    row: &BoardRow,
    run: Uuid,
) -> Result<()> {
    let started = Instant::now();
    let now = deps.clock.now();
    let mut visits = Visits {
        reader,
        db,
        seen: HashSet::new(),
    };
    let Ok(home) = Url::parse(&row.homepage) else {
        return Ok(());
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
    if let Some(url) = old.terms_url.as_deref().and_then(|u| Url::parse(u).ok())
        && let Some(html) = visits.get(&url).await
    {
        let fingerprint = robots::fingerprint(&html);
        if !old.terms_fingerprint.is_empty() && old.terms_fingerprint != fingerprint {
            flag("its terms page changed since the last check");
        }
        terms_fingerprint = fingerprint;
    }
    let report = Report {
        checked_at: Some(now),
        robots,
        terms_fingerprint,
        flags,
        ..old
    };
    sqlx::query(
        "UPDATE sources.job_boards SET report = $2, recheck_after = $3, updated_at = $4 WHERE id = $1",
    )
    .bind(row.id)
    .bind(sqlx::types::Json(serde_json::to_value(&report).map_err(Error::internal)?))
    .bind(now + RECHECK)
    .bind(now)
    .execute(db)
    .await?;
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
            duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            note: Some(note),
            ..Scan::default()
        }],
        deps.clock.now(),
    )
    .await?;
    Ok(())
}
