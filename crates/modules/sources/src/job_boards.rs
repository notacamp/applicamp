//! The job-board registry (`sources.job_boards`): every board a search may
//! read, the candidates discovery found, and the ones declined, each with
//!
//! - its **config**: how to read it, one of three generic adapters (a JSON
//!   API with a field mapping, an RSS or Atom feed with a mapping, JSON-LD
//!   `JobPosting`s through a sitemap) or HN's own extractor, its pages, its
//!   polling interval and its pace ([`BoardConfig`]);
//! - its **terms**: how to credit it, whether its apply links stay
//!   unchanged, whether expired jobs must go, and where those clauses are
//!   ([`Terms`]);
//! - its **report**: what the automatic check found (robots rules, the
//!   terms' sentences, the feed, sample jobs) ([`Report`]).
//!
//! A search reads **approved** boards only; nothing else is ever read for
//! jobs. The seed list (`job_boards.toml`, or the file
//! `APPLICAMP_SOURCES__JOB_BOARDS_FILE` names) is synced at start: new
//! boards are added, the seed's config and terms refreshed unless the owner
//! edited the config; a board's status is the owner's and never reset.

use std::collections::BTreeMap;

use applicamp_kernel::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::SourcesConfig;

/// The seed list built in.
pub const SEED: &str = include_str!("../job_boards.toml");

/// Pages read per fetch, at most.
pub const MAX_PAGES: u32 = 10;
/// The shortest polling interval a board may have, in minutes.
pub const MIN_POLL_MINUTES: u32 = 15;
/// The longest, in minutes (a week).
pub const MAX_POLL_MINUTES: u32 = 7 * 24 * 60;
/// The polling interval of a board whose config names none, in minutes.
pub const DEFAULT_POLL_MINUTES: u32 = 60;
/// Postings a JSON-LD sitemap board reads per fetch, at most.
pub const MAX_SITEMAP_JOBS: u32 = 50;

/// Whether `slug` is a board's name as stored: lower-case letters, digits
/// and `-`, starting with a letter or digit, at most 40.
pub fn is_slug(slug: &str) -> bool {
    let mut chars = slug.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `name` as a slug (`We Work Remotely` → `we-work-remotely`); `None` when
/// nothing is left.
pub fn slug_of(name: &str) -> Option<String> {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-').to_owned();
    is_slug(&slug).then_some(slug)
}

/// A site's host as the registry keeps it: lower case, without `www.`.
pub fn host_of(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
}

/// How a board is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardKind)]
pub enum Kind {
    /// A JSON API, its fields mapped by JSON pointer.
    JsonApi,
    /// An RSS 2.0 or Atom feed, its fields mapped by element name.
    Rss,
    /// `JobPosting`s in JSON-LD, found through a sitemap.
    JsonldSitemap,
    /// HN "Who is hiring": free-text comments the model reads.
    Hn,
}

impl Kind {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsonApi => "json_api",
            Self::Rss => "rss",
            Self::JsonldSitemap => "jsonld_sitemap",
            Self::Hn => "hn",
        }
    }

    /// The kind named `raw`.
    pub fn parse(raw: &str) -> Option<Self> {
        [Self::JsonApi, Self::Rss, Self::JsonldSitemap, Self::Hn]
            .into_iter()
            .find(|k| k.as_str() == raw)
    }
}

/// How much a board matters to a remote search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardPriority)]
pub enum Priority {
    /// Remote-first, open worldwide.
    High,
    /// National, regional or general: kept, never in the way.
    Low,
}

impl Priority {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Low => "low",
        }
    }

    /// `high`, else low.
    pub fn parse(raw: &str) -> Self {
        if raw == "high" { Self::High } else { Self::Low }
    }
}

/// Where a board stands with the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardStatus)]
pub enum Status {
    /// Waiting for the owner: never read for jobs.
    Candidate,
    /// Read by searches.
    Approved,
    /// Approved once, not read for now.
    Paused,
    /// Not to be read, with the reason.
    Declined,
}

impl Status {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Approved => "approved",
            Self::Paused => "paused",
            Self::Declined => "declined",
        }
    }

    /// The status named `raw`; anything unknown is a candidate (never read).
    pub fn parse(raw: &str) -> Self {
        match raw {
            "approved" => Self::Approved,
            "paused" => Self::Paused,
            "declined" => Self::Declined,
            _ => Self::Candidate,
        }
    }
}

/// Where a board came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardOrigin)]
pub enum Origin {
    /// The seed list.
    Seed,
    /// The weekly discovery.
    Discovery,
    /// Someone suggested it.
    Suggested,
}

impl Origin {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Discovery => "discovery",
            Self::Suggested => "suggested",
        }
    }

    /// The origin named `raw`; anything unknown is discovery.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "seed" => Self::Seed,
            "suggested" => Self::Suggested,
            _ => Self::Discovery,
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A cursor-paginated API: the next page's cursor is read from the answer
/// (`from`, a JSON pointer) and sent as the query parameter `param`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub param: String,
    pub from: String,
}

/// Keep only the items whose value at `pointer` is `equals`
/// (Arbeitnow's `remote: true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub pointer: String,
    pub equals: serde_json::Value,
}

/// A feed title that holds the company too ("Example Co: Staff Engineer").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleSplit {
    pub separator: String,
    /// The company comes first.
    #[serde(default)]
    pub company_first: bool,
}

/// Where each normalised field is in an item: a JSON pointer (`/title`)
/// for a JSON API, an element name (`title`, `region`) for a feed. A field
/// left out is not read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fields {
    /// The posting's id on the board; its URL when left out.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    /// The posting on the board (what the app links to, crediting it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The apply link, used unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Who may apply: a list, or text separated by commas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<String>,
    /// HTML or text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Seconds since 1970, RFC 3339, RFC 2822 or `YYYY-MM-DDTHH:MM:SS` (UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_max: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_period: Option<String>,
    /// Pay as written, when the board has no numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_text: Option<String>,
}

/// How to read a board. Validated ([`BoardConfig::validate`]) before it is
/// saved; unknown keys are refused.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardConfig {
    /// The API or the feed (json_api, rss).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<Url>,
    /// The sitemap listing its postings (jsonld_sitemap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sitemap: Option<Url>,
    /// Query parameters the board requires.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    /// Where the jobs are in the answer (a JSON pointer; the answer itself
    /// when left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    /// The answer's first item is not a job (RemoteOK's legal notice).
    #[serde(default, skip_serializing_if = "is_false")]
    pub skip_first: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    /// A page-numbered API's page parameter (from 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_param: Option<String>,
    /// A JSON pointer that is null on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Pages read per fetch (1 to 10; 1 when left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<u32>,
    /// The least time between two fetches, in minutes (15 minutes to a week;
    /// an hour when left out). Within it, searches use the last answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_minutes: Option<u32>,
    /// The least time between two requests to its host, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_interval_ms: Option<u64>,
    /// Every job on it is remote.
    #[serde(default, skip_serializing_if = "is_false")]
    pub remote_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
    #[serde(default)]
    pub fields: Fields,
    /// The pay's currency when the board states none per job (ISO 4217).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_currency: Option<String>,
    /// The pay's period when the board states none per job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salary_period: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_split: Option<TitleSplit>,
    /// Postings read per fetch through a sitemap (at most 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_jobs: Option<u32>,
    /// Only sitemap URLs whose path starts with this are postings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_path: Option<String>,
}

fn pointer_ok(pointer: &str) -> bool {
    pointer.is_empty() || pointer.starts_with('/')
}

fn web_url(url: &Url) -> bool {
    matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

impl BoardConfig {
    /// Pages per fetch.
    pub fn pages(&self) -> u32 {
        self.max_pages.unwrap_or(1).clamp(1, MAX_PAGES)
    }

    /// The polling interval.
    pub fn poll(&self) -> chrono::TimeDelta {
        let minutes = self
            .poll_minutes
            .unwrap_or(DEFAULT_POLL_MINUTES)
            .clamp(MIN_POLL_MINUTES, MAX_POLL_MINUTES);
        chrono::TimeDelta::minutes(i64::from(minutes))
    }

    /// The least time between two requests to its host.
    pub fn spacing(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.min_interval_ms.unwrap_or(0).min(120_000))
    }

    /// The URL a fetch starts at.
    pub fn start(&self, kind: Kind) -> Option<&Url> {
        match kind {
            Kind::JsonApi | Kind::Rss => self.endpoint.as_ref(),
            Kind::JsonldSitemap => self.sitemap.as_ref(),
            Kind::Hn => None,
        }
    }

    /// Whether this config can read a board of `kind`; what is wrong if not.
    pub fn validate(&self, kind: Kind) -> std::result::Result<(), String> {
        for (name, url) in [("endpoint", &self.endpoint), ("sitemap", &self.sitemap)] {
            if let Some(url) = url
                && !web_url(url)
            {
                return Err(format!("`{name}` must be an http(s) address"));
            }
        }
        match kind {
            Kind::JsonApi | Kind::Rss if self.endpoint.is_none() => {
                return Err("`endpoint` is required".to_owned());
            }
            Kind::JsonldSitemap if self.sitemap.is_none() => {
                return Err("`sitemap` is required".to_owned());
            }
            _ => {}
        }
        if let Some(pages) = self.max_pages
            && !(1..=MAX_PAGES).contains(&pages)
        {
            return Err(format!("`max_pages` must be 1 to {MAX_PAGES}"));
        }
        if let Some(poll) = self.poll_minutes
            && !(MIN_POLL_MINUTES..=MAX_POLL_MINUTES).contains(&poll)
        {
            return Err(format!(
                "`poll_minutes` must be {MIN_POLL_MINUTES} to {MAX_POLL_MINUTES}"
            ));
        }
        if let Some(jobs) = self.max_jobs
            && !(1..=MAX_SITEMAP_JOBS).contains(&jobs)
        {
            return Err(format!("`max_jobs` must be 1 to {MAX_SITEMAP_JOBS}"));
        }
        if self.cursor.is_some() && self.page_param.is_some() {
            return Err("use `cursor` or `page_param`, not both".to_owned());
        }
        if kind == Kind::JsonApi {
            let f = &self.fields;
            let pointers = [
                &self.items,
                &self.next,
                &f.reference,
                &f.title,
                &f.company,
                &f.url,
                &f.apply_url,
                &f.location,
                &f.regions,
                &f.description,
                &f.posted_at,
                &f.expires_at,
                &f.salary_min,
                &f.salary_max,
                &f.salary_currency,
                &f.salary_period,
                &f.salary_text,
            ];
            if pointers.into_iter().flatten().any(|p| !pointer_ok(p))
                || self.cursor.as_ref().is_some_and(|c| !pointer_ok(&c.from))
                || self
                    .filter
                    .as_ref()
                    .is_some_and(|f| !pointer_ok(&f.pointer))
            {
                return Err("every field of a JSON API is a JSON pointer (`/title`)".to_owned());
            }
        }
        if matches!(kind, Kind::JsonApi | Kind::Rss)
            && (self.fields.title.is_none() || self.fields.url.is_none())
        {
            return Err("`fields.title` and `fields.url` are required".to_owned());
        }
        for (key, value) in &self.query {
            if key.is_empty() || key.len() > 50 || value.len() > 200 {
                return Err("a `query` parameter is empty or too long".to_owned());
            }
        }
        if let Some(currency) = &self.salary_currency
            && !(currency.len() == 3 && currency.chars().all(|c| c.is_ascii_uppercase()))
        {
            return Err("`salary_currency` is a 3-letter code (`USD`)".to_owned());
        }
        Ok(())
    }
}

/// What a board's terms ask, and where they say it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = BoardTerms)]
pub struct Terms {
    /// How to credit it ("Himalayas").
    #[serde(default)]
    pub attribution: String,
    /// A link back to the posting on the board is required.
    #[serde(default)]
    pub link_back: bool,
    /// That link must be followed (no `nofollow`).
    #[serde(default)]
    pub follow: bool,
    /// The apply link is used as the board gives it.
    #[serde(default)]
    pub apply_unchanged: bool,
    /// Its jobs are never republished elsewhere.
    #[serde(default)]
    pub no_redistribution: bool,
    /// Jobs past their expiry are taken out of view.
    #[serde(default)]
    pub drop_expired: bool,
    /// Its logo is never shown.
    #[serde(default)]
    pub no_logo: bool,
    /// Fetches a day, at most (its polling interval keeps to it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fetches_per_day: Option<u32>,
    /// What else matters, in a sentence or two.
    #[serde(default)]
    pub notes: String,
    /// Where the clauses are.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// One robots.txt rule for our user agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RobotsRule {
    pub allow: bool,
    pub pattern: String,
}

/// What a board's robots.txt says to us.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RobotsReport {
    /// The host it was read from (the feed's, else the site's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// There is one (a 404 allows everything).
    pub found: bool,
    /// The rules for `applicamp`, else for `*`.
    #[serde(default)]
    pub rules: Vec<RobotsRule>,
    /// Seconds between requests it asks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crawl_delay: Option<f64>,
    /// Whether the config's endpoint is allowed (at the check).
    #[serde(default)]
    pub endpoint_allowed: Option<bool>,
    /// The rule that decided it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deciding_rule: Option<String>,
    /// A fingerprint of the file, to notice a change.
    #[serde(default)]
    pub fingerprint: String,
}

/// A job a check read as a sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SampleJob {
    pub title: String,
    pub company: String,
    pub location: Option<String>,
    pub url: String,
    /// Who may apply, as the board says.
    #[serde(default)]
    pub regions: Vec<String>,
}

/// What the automatic check found. Every sentence and job in it is copied
/// from the board's site: data, never instructions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = BoardReport)]
pub struct Report {
    pub checked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub robots: RobotsReport,
    /// The terms page found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms_url: Option<String>,
    /// Its sentences about scraping, robots, automation, aggregation, the
    /// API, linking back, attribution or commercial use, verbatim (at most
    /// 10).
    #[serde(default)]
    pub terms_sentences: Vec<String>,
    /// A fingerprint of the terms page, to notice a change.
    #[serde(default)]
    pub terms_fingerprint: String,
    /// A feed found (`rss`), or an API (`json_api`), or `jsonld_sitemap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found_kind: Option<Kind>,
    /// Where.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found_url: Option<String>,
    /// The homepage has JSON-LD `JobPosting`s.
    #[serde(default)]
    pub jsonld: bool,
    /// Up to 20 jobs parsed with the proposed config.
    #[serde(default)]
    pub sample: Vec<SampleJob>,
    /// Share of the sample that is remote (0–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_share: Option<f64>,
    /// Share of the sample open worldwide (0–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worldwide_share: Option<f64>,
    /// The site's language (its `<html lang>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// How the app would credit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// What the owner should know ("robots.txt disallows the API",
    /// "terms changed since approval").
    #[serde(default)]
    pub flags: Vec<String>,
    /// For the owner, in a sentence (the seed list's notes).
    #[serde(default)]
    pub notes: String,
}

/// A board as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BoardRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub homepage: String,
    pub host: String,
    pub kind: String,
    pub priority: String,
    pub status: String,
    pub origin: String,
    pub country: Option<String>,
    pub config: sqlx::types::Json<serde_json::Value>,
    pub config_edited: bool,
    pub terms: sqlx::types::Json<serde_json::Value>,
    pub report: Option<sqlx::types::Json<serde_json::Value>>,
    pub decline_reason: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub recheck_after: Option<DateTime<Utc>>,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub last_fetch_jobs: Option<i32>,
    pub last_error: Option<String>,
    pub backoff_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// The columns a [`BoardRow`] reads.
macro_rules! board_columns {
    () => {
        "id, slug, name, homepage, host, kind, priority, status, origin, country, config, \
         config_edited, terms, report, decline_reason, approved_at, recheck_after, last_fetch_at, \
         last_fetch_jobs, last_error, backoff_until, created_at"
    };
}
pub(crate) use board_columns;

impl BoardRow {
    /// Its kind; `None` when the row names an unknown one.
    pub fn kind(&self) -> Option<Kind> {
        Kind::parse(&self.kind)
    }

    pub fn status(&self) -> Status {
        Status::parse(&self.status)
    }

    pub fn priority(&self) -> Priority {
        Priority::parse(&self.priority)
    }

    pub fn origin(&self) -> Origin {
        Origin::parse(&self.origin)
    }

    /// Its config; `Err` with what is wrong when it does not read.
    pub fn config(&self) -> std::result::Result<BoardConfig, String> {
        serde_json::from_value(self.config.0.clone()).map_err(|e| e.to_string())
    }

    /// Its terms (defaults for what is missing).
    pub fn terms(&self) -> Terms {
        serde_json::from_value(self.terms.0.clone()).unwrap_or_default()
    }

    /// Its report, if checked.
    pub fn report(&self) -> Option<Report> {
        self.report
            .as_ref()
            .and_then(|r| serde_json::from_value(r.0.clone()).ok())
    }

    /// How its jobs are credited.
    pub fn attribution(&self) -> String {
        let terms = self.terms();
        if terms.attribution.trim().is_empty() {
            self.name.clone()
        } else {
            terms.attribution
        }
    }
}

/// A board of a seed list.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedBoard {
    pub slug: String,
    pub name: String,
    pub homepage: Url,
    pub kind: Kind,
    pub priority: Priority,
    pub status: Status,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub decline_reason: Option<String>,
    #[serde(default)]
    pub config: BoardConfig,
    #[serde(default)]
    pub terms: Terms,
    #[serde(default)]
    pub report: Option<Report>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    #[serde(default)]
    board: Vec<SeedBoard>,
}

/// The boards of a seed list. Every slug is a slug and unique, every
/// approved board's config reads its kind, a declined one says why; the
/// first that is not is the error.
pub fn parse(toml_text: &str) -> std::result::Result<Vec<SeedBoard>, String> {
    let file: SeedFile = toml::from_str(toml_text).map_err(|e| format!("job boards list: {e}"))?;
    let mut seen: Vec<String> = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    for board in &file.board {
        let at = |what: &str| format!("job boards list: `{}`: {what}", board.slug);
        if !is_slug(&board.slug) {
            return Err(at("not a slug (a-z, 0-9, -)"));
        }
        if seen.contains(&board.slug) {
            return Err(at("listed twice"));
        }
        seen.push(board.slug.clone());
        let host = host_of(&board.homepage).ok_or_else(|| at("its homepage has no host"))?;
        if hosts.contains(&host) {
            return Err(at("another board has the same site"));
        }
        hosts.push(host);
        if board.name.trim().is_empty() {
            return Err(at("no name"));
        }
        if let Some(country) = &board.country
            && !(country.len() == 2 && country.chars().all(|c| c.is_ascii_uppercase()))
        {
            return Err(at("`country` is a 2-letter code (`PT`)"));
        }
        if board.status == Status::Declined
            && board.decline_reason.as_deref().is_none_or(str::is_empty)
        {
            return Err(at("a declined board says why (`decline_reason`)"));
        }
        if matches!(board.status, Status::Approved | Status::Paused) {
            board.config.validate(board.kind).map_err(|e| at(&e))?;
        }
    }
    Ok(file.board)
}

/// The seed list `config` names: its file, or the one built in.
pub fn seed(config: &SourcesConfig) -> std::result::Result<Vec<SeedBoard>, String> {
    match &config.job_boards_file {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| {
                format!("APPLICAMP_SOURCES__JOB_BOARDS_FILE={}: {e}", path.display())
            })?;
            parse(&text)
        }
        None => parse(SEED),
    }
}

/// Make the registry hold `seed`: new boards added (with their status),
/// known ones refreshed (name, site, kind, priority, country, terms, and
/// config unless the owner edited it). A board's status, approval and
/// reason stay the owner's. A seed board whose site another board already
/// has is skipped. How many seed boards are in the registry.
pub async fn sync(db: &PgPool, seed: &[SeedBoard], now: DateTime<Utc>) -> Result<usize> {
    let mut count = 0;
    for board in seed {
        let Some(host) = host_of(&board.homepage) else {
            continue;
        };
        let config =
            serde_json::to_value(&board.config).map_err(applicamp_kernel::Error::internal)?;
        let terms =
            serde_json::to_value(&board.terms).map_err(applicamp_kernel::Error::internal)?;
        let report = board
            .report
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(applicamp_kernel::Error::internal)?;
        let updated = sqlx::query(
            "UPDATE sources.job_boards
                SET name = $2, homepage = $3, kind = $4, priority = $5, country = $6,
                    terms = $7, config = CASE WHEN config_edited THEN config ELSE $8 END,
                    report = coalesce(report, $9), updated_at = $10
              WHERE slug = $1 AND host = $11",
        )
        .bind(&board.slug)
        .bind(&board.name)
        .bind(board.homepage.as_str())
        .bind(board.kind.as_str())
        .bind(board.priority.as_str())
        .bind(&board.country)
        .bind(sqlx::types::Json(&terms))
        .bind(sqlx::types::Json(&config))
        .bind(report.as_ref().map(sqlx::types::Json))
        .bind(now)
        .bind(&host)
        .execute(db)
        .await?;
        if updated.rows_affected() == 1 {
            count += 1;
            continue;
        }
        let approved = board.status == Status::Approved;
        let inserted = sqlx::query(
            "INSERT INTO sources.job_boards
                (id, slug, name, homepage, host, kind, priority, status, origin, country, config,
                 terms, report, decline_reason, approved_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'seed', $9, $10, $11, $12, $13, $14, $15, $15)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(&board.slug)
        .bind(&board.name)
        .bind(board.homepage.as_str())
        .bind(&host)
        .bind(board.kind.as_str())
        .bind(board.priority.as_str())
        .bind(board.status.as_str())
        .bind(&board.country)
        .bind(sqlx::types::Json(&config))
        .bind(sqlx::types::Json(&terms))
        .bind(report.as_ref().map(sqlx::types::Json))
        .bind(&board.decline_reason)
        .bind(approved.then_some(now))
        .bind(now)
        .execute(db)
        .await?;
        if inserted.rows_affected() == 1 {
            count += 1;
        } else {
            tracing::warn!(board = %board.slug, "a seed job board's slug or site is taken; skipped");
        }
    }
    Ok(count)
}

/// Every board, newest first within each status.
pub async fn all(db: &PgPool) -> Result<Vec<BoardRow>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards ORDER BY created_at, slug"
    ))
    .fetch_all(db)
    .await?)
}

/// The approved boards: the only ones a search reads.
pub async fn approved(db: &PgPool) -> Result<Vec<BoardRow>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards WHERE status = 'approved' ORDER BY created_at, slug"
    ))
    .fetch_all(db)
    .await?)
}

/// One board.
pub async fn get(db: &PgPool, id: Uuid) -> Result<Option<BoardRow>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        board_columns!(),
        " FROM sources.job_boards WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// What a fetch of `board` did: when, how many jobs, what went wrong, and
/// when it may be asked again after it asked us to slow down.
pub async fn record_fetch(
    db: &PgPool,
    board: Uuid,
    at: DateTime<Utc>,
    jobs: Option<usize>,
    error: Option<&str>,
    backoff_until: Option<DateTime<Utc>>,
) -> Result<()> {
    sqlx::query(
        "UPDATE sources.job_boards
            SET last_fetch_at = CASE WHEN $3::int IS NULL THEN last_fetch_at ELSE $2 END,
                last_fetch_jobs = coalesce($3, last_fetch_jobs),
                last_error = $4,
                backoff_until = coalesce($5, backoff_until)
          WHERE id = $1",
    )
    .bind(board)
    .bind(at)
    .bind(jobs.map(|n| i32::try_from(n).unwrap_or(i32::MAX)))
    .bind(error)
    .bind(backoff_until)
    .execute(db)
    .await?;
    Ok(())
}

/// A job as the board's average salary needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedJob {
    pub source_ref: String,
    pub title: String,
    pub regions: Vec<String>,
    pub engineering: bool,
    pub salary_eur_year: Option<f64>,
    pub posted_at: Option<DateTime<Utc>>,
}

/// Keep what `board` lists now (`sample`: a candidate's check), and forget
/// what it listed more than 30 days ago.
pub async fn keep_listed(
    db: &PgPool,
    board: Uuid,
    jobs: &[ListedJob],
    sample: bool,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    if sample {
        sqlx::query("DELETE FROM sources.board_jobs WHERE board_id = $1 AND sample")
            .bind(board)
            .execute(&mut *tx)
            .await?;
    }
    for job in jobs {
        sqlx::query(
            "INSERT INTO sources.board_jobs
                (board_id, source_ref, title, regions, engineering, salary_eur_year, posted_at,
                 seen_at, sample)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (board_id, source_ref) DO UPDATE
                SET title = excluded.title, regions = excluded.regions,
                    engineering = excluded.engineering, salary_eur_year = excluded.salary_eur_year,
                    posted_at = excluded.posted_at, seen_at = excluded.seen_at,
                    sample = excluded.sample",
        )
        .bind(board)
        .bind(job.source_ref.chars().take(500).collect::<String>())
        .bind(job.title.chars().take(300).collect::<String>())
        .bind(&job.regions)
        .bind(job.engineering)
        .bind(job.salary_eur_year)
        .bind(job.posted_at)
        .bind(now)
        .bind(sample)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "DELETE FROM sources.board_jobs
          WHERE board_id = $1 AND coalesce(posted_at, seen_at) < $2 AND seen_at < $2",
    )
    .bind(board)
    .bind(now - chrono::TimeDelta::days(30))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The jobs every board listed lately, by board.
pub async fn listed(db: &PgPool, since: DateTime<Utc>) -> Result<Vec<(Uuid, ListedJob)>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        board_id: Uuid,
        source_ref: String,
        title: String,
        regions: Vec<String>,
        engineering: bool,
        salary_eur_year: Option<f64>,
        posted_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT board_id, source_ref, title, regions, engineering, salary_eur_year, posted_at
           FROM sources.board_jobs WHERE coalesce(posted_at, seen_at) >= $1",
    )
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.board_id,
                ListedJob {
                    source_ref: r.source_ref,
                    title: r.title,
                    regions: r.regions,
                    engineering: r.engineering,
                    salary_eur_year: r.salary_eur_year,
                    posted_at: r.posted_at,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_list_parses_with_the_first_boards() {
        let boards = parse(SEED).unwrap();
        let status = |slug: &str| boards.iter().find(|b| b.slug == slug).map(|b| b.status);
        for slug in ["himalayas", "jobicy", "remoteok", "arbeitnow", "hn"] {
            assert_eq!(status(slug), Some(Status::Approved), "{slug}");
        }
        for slug in ["landing-jobs", "the-muse", "wellfound", "dice"] {
            assert_eq!(status(slug), Some(Status::Declined), "{slug}");
        }
        for slug in ["we-work-remotely", "working-nomads", "remotive"] {
            assert_eq!(status(slug), Some(Status::Candidate), "{slug}");
        }
        // National and regional boards: kept, low priority.
        for slug in ["itjobs-pt", "net-empregos", "adzuna", "swissdevjobs"] {
            let board = boards.iter().find(|b| b.slug == slug).unwrap();
            assert_eq!(board.priority, Priority::Low, "{slug}");
            assert_eq!(board.status, Status::Candidate, "{slug}");
        }
        // Every approved board is remote-first and says how to credit it.
        for board in boards.iter().filter(|b| b.status == Status::Approved) {
            assert_eq!(board.priority, Priority::High, "{}", board.slug);
            assert!(!board.terms.attribution.is_empty(), "{}", board.slug);
            assert!(board.terms.link_back, "{}", board.slug);
        }
        // Remotive: at most 4 fetches a day.
        let remotive = boards.iter().find(|b| b.slug == "remotive").unwrap();
        assert!(remotive.config.poll() >= chrono::TimeDelta::hours(6));
    }

    #[test]
    fn configs_are_validated() {
        let ok = BoardConfig {
            endpoint: Some("https://jobs.example.com/api".parse().unwrap()),
            fields: Fields {
                title: Some("/title".into()),
                url: Some("/url".into()),
                ..Fields::default()
            },
            ..BoardConfig::default()
        };
        assert!(ok.validate(Kind::JsonApi).is_ok());
        assert!(BoardConfig::default().validate(Kind::JsonApi).is_err());
        let bad_pointer = BoardConfig {
            fields: Fields {
                title: Some("title".into()),
                ..ok.fields.clone()
            },
            ..ok.clone()
        };
        assert!(bad_pointer.validate(Kind::JsonApi).is_err());
        assert!(
            bad_pointer.validate(Kind::Rss).is_ok(),
            "a feed names elements"
        );
        let too_often = BoardConfig {
            poll_minutes: Some(1),
            ..ok.clone()
        };
        assert!(too_often.validate(Kind::JsonApi).is_err());
        let ftp = BoardConfig {
            endpoint: Some("ftp://jobs.example.com/x".parse().unwrap()),
            ..ok
        };
        assert!(ftp.validate(Kind::JsonApi).is_err());
        let unknown: std::result::Result<BoardConfig, _> = serde_json::from_value(
            serde_json::json!({ "endpoint": "https://x.example", "evil": 1 }),
        );
        assert!(unknown.is_err(), "unknown keys are refused");
    }

    #[test]
    fn slugs_and_hosts() {
        assert_eq!(
            slug_of("We Work Remotely").as_deref(),
            Some("we-work-remotely")
        );
        assert_eq!(slug_of("remote | OK").as_deref(), Some("remote-ok"));
        assert_eq!(slug_of("!!!"), None);
        assert!(is_slug("hn") && !is_slug("-x") && !is_slug("A"));
        let url: Url = "https://WWW.Example.com/jobs".parse().unwrap();
        assert_eq!(host_of(&url).as_deref(), Some("example.com"));
    }

    #[test]
    fn bad_seed_entries_are_refused() {
        let board = |extra: &str| {
            format!(
                "[[board]]\nslug = \"x\"\nname = \"X\"\nhomepage = \"https://x.example\"\n\
                 kind = \"json_api\"\npriority = \"high\"\n{extra}"
            )
        };
        assert!(parse(&board("status = \"candidate\"")).is_ok());
        assert!(parse(&board("status = \"declined\"")).is_err(), "no reason");
        assert!(
            parse(&board("status = \"approved\"")).is_err(),
            "no endpoint"
        );
        assert!(parse(&board("status = \"candidate\"\nhost = \"evil.example\"")).is_err());
        let twice = format!(
            "{}\n{}",
            board("status = \"candidate\""),
            board("status = \"candidate\"")
        );
        assert!(parse(&twice).is_err());
    }

    #[sqlx::test(migrations = "../../../migrations")]
    async fn syncing_keeps_the_owners_decisions(db: PgPool) {
        let now = Utc::now();
        let seed = parse(SEED).unwrap();
        let count = sync(&db, &seed, now).await.unwrap();
        assert_eq!(count, seed.len());
        // The owner pauses Himalayas and edits Jobicy's config.
        sqlx::query("UPDATE sources.job_boards SET status = 'paused' WHERE slug = 'himalayas'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE sources.job_boards SET config = '{\"poll_minutes\": 120}', config_edited = true
              WHERE slug = 'jobicy'",
        )
        .execute(&db)
        .await
        .unwrap();
        sync(&db, &seed, now).await.unwrap();
        let boards = all(&db).await.unwrap();
        let by = |slug: &str| boards.iter().find(|b| b.slug == slug).unwrap().clone();
        assert_eq!(by("himalayas").status(), Status::Paused);
        assert_eq!(by("jobicy").config.0["poll_minutes"], 120);
        assert_eq!(by("remoteok").status(), Status::Approved);
        let read: Vec<String> = approved(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|b| b.slug)
            .collect();
        assert!(!read.contains(&"himalayas".to_owned()));
        assert!(
            !read.contains(&"remotive".to_owned()),
            "a candidate is never read"
        );
    }
}
