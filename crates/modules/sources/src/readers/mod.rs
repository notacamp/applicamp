//! Reading jobs from the boards' public APIs, and nothing else.
//!
//! One HTTP client for every board: a 10 s timeout, a 2 MB cap on what it
//! reads, no redirects, `applicamp/<version>` as its user agent, at most
//! [`SourcesConfig::request_spacing`] between two requests to one host, and
//! answers kept in `sources.cache` for [`SourcesConfig::cache_ttl`]. It only
//! ever requests URLs it builds from the configured API bases and checked
//! board tokens and ids: a link a person pastes gives a token and an id,
//! never a host (no SSRF).

pub mod arbeitnow;
pub mod ashby;
pub mod greenhouse;
pub mod hn;
pub mod lever;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use chrono::Utc;
use regex::Regex;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use url::Url;

use crate::{
    config::SourcesConfig,
    job::{Job, Source},
};

/// Longest answer read, in bytes.
pub const MAX_BODY: usize = 2 * 1024 * 1024;
/// Longest Arbeitnow page read: its pages hold 300 jobs with their
/// descriptions, a little over 2 MB.
pub const MAX_ARBEITNOW_BODY: usize = 4 * 1024 * 1024;
/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Arbeitnow pages read per search.
pub const ARBEITNOW_PAGES: u32 = 5;

/// A board token or a posting id as a link or the seed list gives it.
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Whether `token` is a safe board token or posting id: letters, digits,
/// `_`, `.`, `-`, starting with a letter or digit, at most 100.
pub fn is_token(token: &str) -> bool {
    TOKEN.is_match(token) && !token.contains("..")
}

/// A company's board on an applicant tracking system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    /// Greenhouse, Lever or Ashby.
    pub kind: Source,
    /// The board's token (`https://jobs.lever.co/<token>`).
    pub token: String,
    /// The company's name.
    pub company: String,
    /// A Lever board on Lever's EU instance.
    pub eu: bool,
}

/// Why a board could not be read. Says what, never the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The board answered with this status.
    Status(u16),
    /// It could not be reached, or timed out.
    Transport,
    /// Its answer is over [`MAX_BODY`].
    TooLarge,
    /// Its answer is not what the API documents.
    Unexpected,
    /// The cache could not be read or written.
    Cache,
    /// The URL could not be built (a bad token).
    BadUrl,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status(status) => write!(f, "the board answered {status}"),
            Self::Transport => f.write_str("the board could not be reached"),
            Self::TooLarge => f.write_str("the board's answer is over 2 MB"),
            Self::Unexpected => f.write_str("the board's answer is not as documented"),
            Self::Cache => f.write_str("the cache is unavailable"),
            Self::BadUrl => f.write_str("the board's address could not be built"),
        }
    }
}

/// What a search read.
#[derive(Debug, Default)]
pub struct Fetched {
    /// The jobs.
    pub jobs: Vec<Job>,
    /// Boards (and pages) read.
    pub read: u32,
    /// Boards (and pages) that failed.
    pub failed: u32,
}

impl Fetched {
    fn add(&mut self, result: Result<Vec<Job>, FetchError>, what: &str) {
        match result {
            Ok(jobs) => {
                self.read += 1;
                self.jobs.extend(jobs);
            }
            Err(e) => {
                self.failed += 1;
                // The board's kind and the error only: no URL, no answer.
                tracing::info!(source = what, error = %e, "a job board could not be read");
            }
        }
    }

    fn merge(&mut self, other: Self) {
        self.jobs.extend(other.jobs);
        self.read += other.read;
        self.failed += other.failed;
    }
}

/// Reads the boards. Cheap to clone.
#[derive(Clone)]
pub struct Reader {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    config: Arc<SourcesConfig>,
    /// When each host may be asked next.
    next: tokio::sync::Mutex<HashMap<String, Instant>>,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader").finish_non_exhaustive()
    }
}

impl Reader {
    /// A reader over `config`'s boards.
    pub fn new(config: Arc<SourcesConfig>) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("applicamp/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("the job-board client could not be built: {e}"))?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                config,
                next: tokio::sync::Mutex::new(HashMap::new()),
            }),
        })
    }

    /// The configuration it reads with.
    pub fn config(&self) -> &SourcesConfig {
        &self.inner.config
    }

    /// Wait until `host` may be asked again.
    async fn wait_turn(&self, host: &str) {
        let spacing = self.inner.config.request_spacing();
        let wait = {
            let mut next = self.inner.next.lock().await;
            let now = Instant::now();
            let at = next.get(host).copied().unwrap_or(now).max(now);
            next.insert(host.to_owned(), at + spacing);
            at - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// `url`'s answer (at most [`MAX_BODY`]), from the cache while it is
    /// fresh. `Ok(None)` when the board says there is no such thing (404).
    pub async fn get(&self, db: &PgPool, url: &Url) -> Result<Option<Vec<u8>>, FetchError> {
        self.get_at_most(db, url, MAX_BODY).await
    }

    /// [`get`](Self::get), reading at most `max` bytes.
    async fn get_at_most(
        &self,
        db: &PgPool,
        url: &Url,
        max: usize,
    ) -> Result<Option<Vec<u8>>, FetchError> {
        let ttl = self.inner.config.cache_ttl();
        if !ttl.is_zero() {
            let fresh_since =
                Utc::now() - chrono::TimeDelta::from_std(ttl).unwrap_or(chrono::TimeDelta::zero());
            let cached: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT body FROM sources.cache WHERE url = $1 AND fetched_at > $2",
            )
            .bind(url.as_str())
            .bind(fresh_since)
            .fetch_optional(db)
            .await
            .map_err(|_| FetchError::Cache)?;
            if let Some(body) = cached {
                return Ok(Some(body));
            }
        }
        self.wait_turn(url.host_str().unwrap_or_default()).await;
        let mut response = self
            .inner
            .http
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| FetchError::Transport)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > max as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| FetchError::Transport)? {
            if body.len() + chunk.len() > max {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        if !ttl.is_zero() {
            sqlx::query(
                "INSERT INTO sources.cache (url, fetched_at, body) VALUES ($1, $2, $3)
                 ON CONFLICT (url) DO UPDATE
                    SET fetched_at = excluded.fetched_at, body = excluded.body",
            )
            .bind(url.as_str())
            .bind(Utc::now())
            .bind(&body)
            .execute(db)
            .await
            .map_err(|_| FetchError::Cache)?;
        }
        Ok(Some(body))
    }

    /// `url`'s answer as `T`; `Ok(None)` on a 404.
    async fn get_json<T: DeserializeOwned>(
        &self,
        db: &PgPool,
        url: &Url,
    ) -> Result<Option<T>, FetchError> {
        self.get_json_at_most(db, url, MAX_BODY).await
    }

    /// [`get_json`](Self::get_json), reading at most `max` bytes.
    async fn get_json_at_most<T: DeserializeOwned>(
        &self,
        db: &PgPool,
        url: &Url,
        max: usize,
    ) -> Result<Option<T>, FetchError> {
        match self.get_at_most(db, url, max).await? {
            None => Ok(None),
            Some(body) => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| FetchError::Unexpected),
        }
    }

    /// `base` with `segments` appended (each percent-encoded) and `query`.
    fn url(base: &Url, segments: &[&str], query: &[(&str, &str)]) -> Result<Url, FetchError> {
        if segments
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
        {
            return Err(FetchError::BadUrl);
        }
        let mut url = base.clone();
        url.path_segments_mut()
            .map_err(|()| FetchError::BadUrl)?
            .pop_if_empty()
            .extend(segments);
        if query.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(query);
        }
        Ok(url)
    }

    /// The jobs on `board`.
    pub async fn board(&self, db: &PgPool, board: &Board) -> Result<Vec<Job>, FetchError> {
        if !is_token(&board.token) {
            return Err(FetchError::BadUrl);
        }
        let config = &self.inner.config;
        let jobs = match board.kind {
            Source::Greenhouse => {
                let url = Self::url(
                    &config.greenhouse_url,
                    &["v1", "boards", board.token.as_str(), "jobs"],
                    &[],
                )?;
                let list: Option<greenhouse::List> = self.get_json(db, &url).await?;
                list.map(|list| greenhouse::jobs(list, board))
            }
            Source::Lever => {
                let base = if board.eu {
                    &config.lever_eu_url
                } else {
                    &config.lever_url
                };
                let url = Self::url(
                    base,
                    &["v0", "postings", board.token.as_str()],
                    &[("mode", "json")],
                )?;
                let list: Option<Vec<lever::Posting>> = self.get_json(db, &url).await?;
                list.map(|list| lever::jobs(list, board))
            }
            Source::Ashby => {
                let url = Self::url(
                    &config.ashby_url,
                    &["posting-api", "job-board", board.token.as_str()],
                    &[("includeCompensation", "true")],
                )?;
                let list: Option<ashby::List> = self.get_json(db, &url).await?;
                list.map(|list| ashby::jobs(list, board))
            }
            Source::Arbeitnow | Source::Hn => return Err(FetchError::BadUrl),
        };
        jobs.ok_or(FetchError::Status(404))
    }

    /// Arbeitnow's newest jobs, up to [`ARBEITNOW_PAGES`] pages.
    pub async fn arbeitnow(&self, db: &PgPool) -> Fetched {
        let mut fetched = Fetched::default();
        for page in 1..=ARBEITNOW_PAGES {
            let page = page.to_string();
            let result = async {
                let url = Self::url(
                    &self.inner.config.arbeitnow_url,
                    &["api", "job-board-api"],
                    &[("page", page.as_str())],
                )?;
                let answer: Option<arbeitnow::Page> =
                    self.get_json_at_most(db, &url, MAX_ARBEITNOW_BODY).await?;
                answer.ok_or(FetchError::Status(404))
            }
            .await;
            match result {
                Ok(answer) => {
                    let last = answer.links.next.is_none() || answer.data.is_empty();
                    fetched.add(Ok(arbeitnow::jobs(answer)), "arbeitnow");
                    if last {
                        break;
                    }
                }
                Err(e) => {
                    fetched.add(Err(e), "arbeitnow");
                    break;
                }
            }
        }
        fetched
    }

    /// The top-level comments of the latest "Ask HN: Who is hiring?".
    pub async fn hn(&self, db: &PgPool) -> Fetched {
        let mut fetched = Fetched::default();
        let result = async {
            let base = &self.inner.config.hn_url;
            let url = Self::url(
                base,
                &["api", "v1", "search_by_date"],
                &[("tags", "story,author_whoishiring"), ("hitsPerPage", "10")],
            )?;
            let stories: hn::Stories = self
                .get_json(db, &url)
                .await?
                .ok_or(FetchError::Status(404))?;
            let Some(story) = hn::latest_hiring(&stories) else {
                return Ok::<Vec<Job>, FetchError>(Vec::new());
            };
            let mut jobs = Vec::new();
            for page in 0..hn::PAGES {
                let tags = format!("comment,story_{}", story.id);
                let page = page.to_string();
                let per_page = hn::PER_PAGE.to_string();
                let url = Self::url(
                    base,
                    &["api", "v1", "search_by_date"],
                    &[
                        ("tags", tags.as_str()),
                        ("hitsPerPage", per_page.as_str()),
                        ("page", page.as_str()),
                    ],
                )?;
                let comments: hn::Comments = self
                    .get_json(db, &url)
                    .await?
                    .ok_or(FetchError::Status(404))?;
                let more = comments.page + 1 < comments.nb_pages;
                jobs.extend(hn::jobs(comments, &story));
                if !more {
                    break;
                }
            }
            Ok::<Vec<Job>, FetchError>(jobs)
        }
        .await;
        fetched.add(result, "hn");
        fetched
    }

    /// The jobs on `boards` and, when enabled, Arbeitnow and HN. The
    /// sources are read side by side, each board after the other; a board
    /// that fails is counted and skipped.
    pub async fn all(&self, db: &PgPool, boards: &[Board]) -> Fetched {
        let config = &self.inner.config;
        let group = |kind: Source| {
            let boards: Vec<&Board> = boards
                .iter()
                .filter(|b| b.kind == kind && config.is_enabled(kind))
                .collect();
            async move {
                let mut fetched = Fetched::default();
                for board in boards {
                    fetched.add(self.board(db, board).await, kind.as_str());
                }
                fetched
            }
        };
        let arbeitnow = async {
            if config.is_enabled(Source::Arbeitnow) {
                self.arbeitnow(db).await
            } else {
                Fetched::default()
            }
        };
        let hn = async {
            if config.is_enabled(Source::Hn) {
                self.hn(db).await
            } else {
                Fetched::default()
            }
        };
        let (a, b, c, d, e) = tokio::join!(
            group(Source::Greenhouse),
            group(Source::Lever),
            group(Source::Ashby),
            arbeitnow,
            hn
        );
        let mut fetched = Fetched::default();
        for part in [a, b, c, d, e] {
            fetched.merge(part);
        }
        fetched
    }

    /// Fill in the description of a Greenhouse job, whose list leaves it
    /// out. Other jobs, and ones already described, are left alone.
    pub async fn describe(&self, db: &PgPool, job: &mut Job) -> Result<(), FetchError> {
        if job.source != Source::Greenhouse || !job.description.is_empty() {
            return Ok(());
        }
        let (token, id) = job.source_ref.split_once(':').ok_or(FetchError::BadUrl)?;
        if !is_token(token) || !is_token(id) {
            return Err(FetchError::BadUrl);
        }
        let url = Self::url(
            &self.inner.config.greenhouse_url,
            &["v1", "boards", token, "jobs", id],
            &[],
        )?;
        let detail: greenhouse::Detail = self
            .get_json(db, &url)
            .await?
            .ok_or(FetchError::Status(404))?;
        job.description = greenhouse::description(&detail);
        Ok(())
    }

    /// The posting a person pasted, when `link` is one on a Greenhouse,
    /// Lever or Ashby board, read through that board's API; with the board
    /// (company `company`, when the caller knows it). `Ok(None)` for any
    /// other link, which is never fetched.
    pub async fn posting(
        &self,
        db: &PgPool,
        link: &Url,
        company: impl Fn(Source, &str) -> Option<String>,
    ) -> Result<Option<(Job, Board)>, FetchError> {
        let Some((kind, token, id, eu)) = posting_link(link) else {
            return Ok(None);
        };
        let mut board = Board {
            company: company(kind, &token).unwrap_or_else(|| token.clone()),
            kind,
            token,
            eu,
        };
        let config = &self.inner.config;
        let job = match kind {
            Source::Greenhouse => {
                let url = Self::url(
                    &config.greenhouse_url,
                    &["v1", "boards", board.token.as_str(), "jobs", id.as_str()],
                    &[],
                )?;
                let detail: Option<greenhouse::Detail> = self.get_json(db, &url).await?;
                detail.map(|detail| greenhouse::job_of_detail(detail, &mut board))
            }
            Source::Lever => {
                let base = if eu {
                    &config.lever_eu_url
                } else {
                    &config.lever_url
                };
                let url = Self::url(
                    base,
                    &["v0", "postings", board.token.as_str(), id.as_str()],
                    &[("mode", "json")],
                )?;
                let posting: Option<lever::Posting> = self.get_json(db, &url).await?;
                posting.and_then(|posting| lever::job(posting, &board))
            }
            Source::Ashby => self
                .board(db, &board)
                .await?
                .into_iter()
                .find(|job| job.source_ref == id),
            Source::Arbeitnow | Source::Hn => None,
        };
        Ok(job.map(|job| (job, board)))
    }
}

/// The board, token and posting id of a link to a Greenhouse, Lever or
/// Ashby posting (`(kind, token, id, on Lever's EU instance)`); `None` for
/// anything else.
pub fn posting_link(link: &Url) -> Option<(Source, String, String, bool)> {
    if !matches!(link.scheme(), "https" | "http") {
        return None;
    }
    let host = link.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = link.path_segments()?.filter(|s| !s.is_empty()).collect();
    let (kind, token, id, eu) = match (host.as_str(), segments.as_slice()) {
        ("boards.greenhouse.io" | "job-boards.greenhouse.io", [token, "jobs", id, ..]) => {
            (Source::Greenhouse, *token, *id, false)
        }
        ("jobs.lever.co", [token, id, ..]) => (Source::Lever, *token, *id, false),
        ("jobs.eu.lever.co", [token, id, ..]) => (Source::Lever, *token, *id, true),
        ("jobs.ashbyhq.com", [token, id, ..]) => (Source::Ashby, *token, *id, false),
        _ => return None,
    };
    if kind == Source::Greenhouse && !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    (is_token(token) && is_token(id)).then(|| {
        let id = if kind == Source::Greenhouse {
            id.to_owned()
        } else {
            id.to_ascii_lowercase()
        };
        (kind, token.to_owned(), id, eu)
    })
}
