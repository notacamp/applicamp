//! Reading jobs from the boards' public APIs and feeds, and nothing else.
//!
//! - **Company boards** (Greenhouse, Lever, Ashby; off by default, kept for
//!   company watchers, #20) and HN: one HTTP client, a 10 s timeout, a 2 MB
//!   cap, no redirects, `applicamp/<version>` as its user agent, at most
//!   [`SourcesConfig::request_spacing`] between two requests to one host,
//!   answers kept in `sources.cache` for [`SourcesConfig::cache_ttl`]. Only
//!   URLs built from the configured API bases and checked board tokens and
//!   ids: a link a person pastes gives a token and an id, never a host.
//! - **Job boards of the registry** ([`crate::job_boards`]), approved only:
//!   read through their config by one of three generic adapters (a JSON API
//!   with a field mapping, an RSS or Atom feed, JSON-LD postings through a
//!   sitemap) or HN's extractor; a 4 MB cap; their own pace on top of the
//!   instance's; never twice within their polling interval (searches then
//!   use the last answer, from the cache); a 429 or 503 backs off for its
//!   `Retry-After`; and a client that reaches only public hosts
//!   ([`crate::net`]).

pub mod ashby;
pub mod greenhouse;
pub mod hn;
pub mod jsonld;
pub mod lever;
pub mod mapping;
pub mod rss;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use url::Url;

use crate::{
    config::SourcesConfig,
    job::{BoardJob, Job, Source, SourceLink},
    job_boards::{self, BoardConfig, BoardRow, Kind, ListedJob},
    net, pay, robots,
};

use mapping::BoardRef;

/// Longest answer read from a company board or HN, in bytes.
pub const MAX_BODY: usize = 2 * 1024 * 1024;
/// Longest answer read from a job board: Arbeitnow's pages hold 300 jobs
/// with their descriptions, a little over 2 MB.
pub const MAX_BOARD_BODY: usize = 4 * 1024 * 1024;
/// Longest page a board's check reads.
pub const MAX_PAGE: usize = 1024 * 1024;
/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// How Applicamp introduces itself to every board.
pub const USER_AGENT: &str = concat!(
    "applicamp/",
    env!("CARGO_PKG_VERSION"),
    " (+https://applicamp.not-a-camp.com)"
);
/// How long a board that answered 429 or 503 without `Retry-After` is left
/// alone, and the longest any `Retry-After` is honoured.
pub const BACKOFF: (Duration, Duration) = (Duration::from_secs(3_600), Duration::from_secs(86_400));

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
    /// It asked us to slow down (429 or 503), for this many seconds if it
    /// said.
    RateLimited(Option<u64>),
    /// It asked us to slow down earlier, and that time is not over.
    BackingOff,
    /// Its address is not a public https one.
    Refused,
    /// Its config does not read.
    BadConfig,
}

impl FetchError {
    /// A short code for the Scanned view and the logs.
    pub fn kind(&self) -> String {
        match self {
            Self::Status(status) => format!("http_{status}"),
            Self::Transport => "unreachable".to_owned(),
            Self::TooLarge => "too_large".to_owned(),
            Self::Unexpected => "unexpected_answer".to_owned(),
            Self::Cache => "cache".to_owned(),
            Self::BadUrl => "bad_url".to_owned(),
            Self::RateLimited(_) => "rate_limited".to_owned(),
            Self::BackingOff => "backing_off".to_owned(),
            Self::Refused => "refused_host".to_owned(),
            Self::BadConfig => "bad_config".to_owned(),
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Status(status) => write!(f, "the board answered {status}"),
            Self::Transport => f.write_str("the board could not be reached"),
            Self::TooLarge => f.write_str("the board's answer is too large"),
            Self::Unexpected => f.write_str("the board's answer is not as documented"),
            Self::Cache => f.write_str("the cache is unavailable"),
            Self::BadUrl => f.write_str("the board's address could not be built"),
            Self::RateLimited(_) => f.write_str("the board asked to slow down"),
            Self::BackingOff => f.write_str("the board asked to slow down earlier"),
            Self::Refused => f.write_str("the board's address is not a public https one"),
            Self::BadConfig => f.write_str("the board's config does not read"),
        }
    }
}

/// An answer, and whether it came from the cache.
#[derive(Debug)]
struct Got {
    body: Vec<u8>,
    cached: bool,
}

/// How reading a job board went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Read now.
    Read,
    /// Within its polling interval: its last answer was used.
    SkippedPollLimit,
    /// It could not be read.
    Error,
}

impl Outcome {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::SkippedPollLimit => "skipped_poll_limit",
            Self::Error => "error",
        }
    }
}

/// What reading one job board gave.
#[derive(Debug, Clone)]
pub struct BoardRead {
    pub board: BoardRef,
    pub jobs: Vec<Job>,
    pub outcome: Outcome,
    /// What went wrong ([`FetchError::kind`]).
    pub error: Option<String>,
    pub duration_ms: u32,
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
    /// Company boards and HN: fixed hosts.
    http: reqwest::Client,
    /// Job boards of the registry: public hosts only (unless allowed).
    boards_http: reqwest::Client,
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
        let builder = || {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .connect_timeout(TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(USER_AGENT)
        };
        let http = builder()
            .build()
            .map_err(|e| format!("the job-board client could not be built: {e}"))?;
        let boards = if config.allow_private_hosts() {
            builder()
        } else {
            builder().dns_resolver(net::PublicResolver)
        };
        let boards_http = boards
            .build()
            .map_err(|e| format!("the job-board client could not be built: {e}"))?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                boards_http,
                config,
                next: tokio::sync::Mutex::new(HashMap::new()),
            }),
        })
    }

    /// The configuration it reads with.
    pub fn config(&self) -> &SourcesConfig {
        &self.inner.config
    }

    /// Wait until `host` may be asked again, `spacing` after the last time
    /// (at least the instance's spacing).
    async fn wait_turn(&self, host: &str, spacing: Duration) {
        let spacing = spacing.max(self.inner.config.request_spacing());
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

    /// `url`'s answer through `client` (at most `max` bytes), from the cache
    /// while it is younger than `ttl`; `spacing` between two requests to its
    /// host. `Ok(None)` when the board says there is no such thing (404).
    async fn fetch(
        &self,
        client: &reqwest::Client,
        db: &PgPool,
        url: &Url,
        max: usize,
        ttl: Duration,
        spacing: Duration,
    ) -> Result<Option<Got>, FetchError> {
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
                return Ok(Some(Got { body, cached: true }));
            }
        }
        self.wait_turn(url.host_str().unwrap_or_default(), spacing)
            .await;
        let mut response = client
            .get(url.clone())
            .header(
                reqwest::header::ACCEPT,
                "application/json, application/rss+xml, application/xml;q=0.9, */*;q=0.8",
            )
            .send()
            .await
            .map_err(|_| FetchError::Transport)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            return Err(FetchError::RateLimited(retry_after));
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
        Ok(Some(Got {
            body,
            cached: false,
        }))
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
        let config = &self.inner.config;
        Ok(self
            .fetch(
                &self.inner.http,
                db,
                url,
                max,
                config.cache_ttl(),
                Duration::ZERO,
            )
            .await?
            .map(|got| got.body))
    }

    /// `url`'s answer as `T`; `Ok(None)` on a 404.
    async fn get_json<T: DeserializeOwned>(
        &self,
        db: &PgPool,
        url: &Url,
    ) -> Result<Option<T>, FetchError> {
        match self.get_at_most(db, url, MAX_BODY).await? {
            None => Ok(None),
            Some(body) => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| FetchError::Unexpected),
        }
    }

    /// One page a board's check reads (robots.txt, its site, its terms, a
    /// feed): a public https address only (unless allowed), never from the
    /// cache, at most [`MAX_PAGE`], a second apart on one host. `Ok(None)`
    /// on a 404.
    pub async fn visit(&self, db: &PgPool, url: &Url) -> Result<Option<Vec<u8>>, FetchError> {
        let allow_private = self.inner.config.allow_private_hosts();
        net::check(url, allow_private).map_err(|_| FetchError::Refused)?;
        let spacing = if allow_private {
            Duration::ZERO
        } else {
            Duration::from_secs(1)
        };
        Ok(self
            .fetch(
                &self.inner.boards_http,
                db,
                url,
                MAX_PAGE,
                Duration::ZERO,
                spacing,
            )
            .await?
            .map(|got| got.body))
    }

    /// A job board's answer at `url`: a public https address only (unless
    /// allowed), at most [`MAX_BOARD_BODY`], reused for `ttl`.
    async fn get_board(
        &self,
        db: &PgPool,
        url: &Url,
        ttl: Duration,
        spacing: Duration,
    ) -> Result<Got, FetchError> {
        net::check(url, self.inner.config.allow_private_hosts())
            .map_err(|_| FetchError::Refused)?;
        self.fetch(
            &self.inner.boards_http,
            db,
            url,
            MAX_BOARD_BODY,
            ttl,
            spacing,
        )
        .await?
        .ok_or(FetchError::Status(404))
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
            Source::Arbeitnow | Source::Hn | Source::Board => return Err(FetchError::BadUrl),
        };
        jobs.ok_or(FetchError::Status(404))
    }

    /// The top-level comments of the latest "Ask HN: Who is hiring?",
    /// reused for the instance's cache time.
    pub async fn hn(&self, db: &PgPool) -> Fetched {
        let mut fetched = Fetched::default();
        let ttl = self.inner.config.cache_ttl();
        fetched.add(self.hn_with(db, ttl).await.map(|(jobs, _)| jobs), "hn");
        fetched
    }

    /// HN's jobs, each answer reused for `ttl`; and whether every answer came
    /// from the cache.
    async fn hn_with(&self, db: &PgPool, ttl: Duration) -> Result<(Vec<Job>, bool), FetchError> {
        let base = &self.inner.config.hn_url;
        let mut cached = true;
        let mut get = async |url: Url| -> Result<Vec<u8>, FetchError> {
            let got = self
                .fetch(&self.inner.http, db, &url, MAX_BODY, ttl, Duration::ZERO)
                .await?
                .ok_or(FetchError::Status(404))?;
            cached &= got.cached;
            Ok(got.body)
        };
        let url = Self::url(
            base,
            &["api", "v1", "search_by_date"],
            &[("tags", "story,author_whoishiring"), ("hitsPerPage", "10")],
        )?;
        let stories: hn::Stories =
            serde_json::from_slice(&get(url).await?).map_err(|_| FetchError::Unexpected)?;
        let Some(story) = hn::latest_hiring(&stories) else {
            return Ok((Vec::new(), cached));
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
            let comments: hn::Comments =
                serde_json::from_slice(&get(url).await?).map_err(|_| FetchError::Unexpected)?;
            let more = comments.page + 1 < comments.nb_pages;
            jobs.extend(hn::jobs(comments, &story));
            if !more {
                break;
            }
        }
        Ok((jobs, cached))
    }

    /// The jobs on the company `boards` whose kind is enabled, each board
    /// after the other; a board that fails is counted and skipped.
    pub async fn all(&self, db: &PgPool, boards: &[Board]) -> Fetched {
        let config = &self.inner.config;
        let group = |kind: Source| {
            let boards: Vec<&Board> = boards
                .iter()
                .filter(|b| b.kind == kind && config.is_enabled(kind.as_str()))
                .collect();
            async move {
                let mut fetched = Fetched::default();
                for board in boards {
                    fetched.add(self.board(db, board).await, kind.as_str());
                }
                fetched
            }
        };
        let (a, b, c) = tokio::join!(
            group(Source::Greenhouse),
            group(Source::Lever),
            group(Source::Ashby)
        );
        let mut fetched = Fetched::default();
        for part in [a, b, c] {
            fetched.merge(part);
        }
        fetched
    }

    /// Read the approved job board `row`, then keep what it did on the
    /// board (when it was read now) and the jobs it lists (for its average
    /// salary). A read whose answers all came from the cache is
    /// [`Outcome::SkippedPollLimit`], unless the board was read since
    /// `fresh_since` (by this search, or the nightly reading just before it).
    pub async fn job_board(
        &self,
        db: &PgPool,
        row: &BoardRow,
        now: DateTime<Utc>,
        fresh_since: DateTime<Utc>,
    ) -> BoardRead {
        let started = Instant::now();
        let board = BoardRef::of(row);
        let result = self.read_board(db, row, &board, now).await;
        let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        let (jobs, outcome, error) = match result {
            Ok((jobs, cached)) => {
                if cached {
                    let read_lately = row.last_fetch_at.is_some_and(|at| at >= fresh_since);
                    let outcome = if read_lately {
                        Outcome::Read
                    } else {
                        Outcome::SkippedPollLimit
                    };
                    (jobs, outcome, None)
                } else {
                    let kept = async {
                        job_boards::record_fetch(db, row.id, now, Some(jobs.len()), None, None)
                            .await?;
                        job_boards::keep_listed(db, row.id, &listed(&jobs), false, now).await
                    }
                    .await;
                    if let Err(e) = kept {
                        tracing::warn!(board = %row.slug, error = %e, "a job board's reading could not be kept");
                    }
                    (jobs, Outcome::Read, None)
                }
            }
            Err(e) => {
                let backoff = match e {
                    FetchError::RateLimited(secs) => {
                        let wait = secs.map_or(BACKOFF.0, Duration::from_secs).min(BACKOFF.1);
                        chrono::TimeDelta::from_std(wait).ok().map(|d| now + d)
                    }
                    _ => None,
                };
                let kind = e.kind();
                // The board and the error only: no URL, no answer.
                tracing::info!(board = %row.slug, error = %kind, "a job board could not be read");
                if let Err(e) =
                    job_boards::record_fetch(db, row.id, now, None, Some(&kind), backoff).await
                {
                    tracing::warn!(board = %row.slug, error = %e, "a job board's error could not be kept");
                }
                (Vec::new(), Outcome::Error, Some(kind))
            }
        };
        BoardRead {
            board,
            jobs,
            outcome,
            error,
            duration_ms,
        }
    }

    /// The jobs of the approved boards `rows`, one board after the other.
    pub async fn job_boards(
        &self,
        db: &PgPool,
        rows: &[BoardRow],
        now: DateTime<Utc>,
        fresh_since: DateTime<Utc>,
    ) -> Vec<BoardRead> {
        let mut reads = Vec::with_capacity(rows.len());
        for row in rows
            .iter()
            .filter(|r| self.inner.config.is_enabled(&r.slug))
        {
            reads.push(self.job_board(db, row, now, fresh_since).await);
        }
        reads
    }

    /// `row`'s jobs, and whether every answer came from the cache.
    async fn read_board(
        &self,
        db: &PgPool,
        row: &BoardRow,
        board: &BoardRef,
        now: DateTime<Utc>,
    ) -> Result<(Vec<Job>, bool), FetchError> {
        if row.backoff_until.is_some_and(|until| until > now) {
            return Err(FetchError::BackingOff);
        }
        let config = row.config().map_err(|_| FetchError::BadConfig)?;
        let kind = row.kind().ok_or(FetchError::BadConfig)?;
        config.validate(kind).map_err(|_| FetchError::BadConfig)?;
        let ttl = config.poll().to_std().unwrap_or(Duration::from_secs(3_600));
        match kind {
            Kind::JsonApi => self.json_api(db, &config, board, ttl).await,
            Kind::Rss => self.feed(db, &config, board, ttl).await,
            Kind::JsonldSitemap => self.sitemap(db, &config, row, board, ttl).await,
            Kind::Hn => {
                let (mut jobs, cached) = self.hn_with(db, ttl).await?;
                for job in &mut jobs {
                    job.board = Some(BoardJob {
                        slug: board.slug.clone(),
                        links: vec![SourceLink {
                            board: board.slug.clone(),
                            name: board.name.clone(),
                            url: job.url.clone(),
                        }],
                        ..BoardJob::default()
                    });
                }
                Ok((jobs, cached))
            }
        }
    }

    /// A JSON API's jobs, page after page (by cursor or page number).
    async fn json_api(
        &self,
        db: &PgPool,
        config: &BoardConfig,
        board: &BoardRef,
        ttl: Duration,
    ) -> Result<(Vec<Job>, bool), FetchError> {
        let endpoint = config.endpoint.as_ref().ok_or(FetchError::BadConfig)?;
        let mut jobs = Vec::new();
        let mut cached = true;
        let mut cursor: Option<String> = None;
        for page in 1..=config.pages() {
            let mut url = endpoint.clone();
            {
                let mut pairs = url.query_pairs_mut();
                for (key, value) in &config.query {
                    pairs.append_pair(key, value);
                }
                if let Some(param) = &config.page_param {
                    pairs.append_pair(param, &page.to_string());
                }
                if let (Some(c), Some(value)) = (&config.cursor, &cursor) {
                    pairs.append_pair(&c.param, value);
                }
            }
            if url.query() == Some("") {
                url.set_query(None);
            }
            let got = self.get_board(db, &url, ttl, config.spacing()).await?;
            cached &= got.cached;
            let answer: serde_json::Value =
                serde_json::from_slice(&got.body).map_err(|_| FetchError::Unexpected)?;
            let items = match config.items.as_deref() {
                Some(pointer) if !pointer.is_empty() => answer.pointer(pointer),
                _ => Some(&answer),
            }
            .and_then(serde_json::Value::as_array)
            .ok_or(FetchError::Unexpected)?;
            let skip = usize::from(config.skip_first && page == 1);
            let before = jobs.len();
            jobs.extend(
                items
                    .iter()
                    .skip(skip)
                    .filter(|item| mapping::kept(item, config))
                    .filter_map(|item| mapping::job_of(item, config, board)),
            );
            let more = if let Some(c) = &config.cursor {
                cursor = answer
                    .pointer(&c.from)
                    .and_then(serde_json::Value::as_str)
                    .filter(|v| !v.is_empty() && v.len() <= 500)
                    .map(str::to_owned);
                cursor.is_some()
            } else if config.page_param.is_some() {
                let last = config
                    .next
                    .as_deref()
                    .is_some_and(|p| answer.pointer(p).is_none_or(serde_json::Value::is_null));
                !last && items.len() > skip
            } else {
                false
            };
            if !more || (jobs.len() == before && items.is_empty()) {
                break;
            }
        }
        Ok((jobs, cached))
    }

    /// A feed's jobs.
    async fn feed(
        &self,
        db: &PgPool,
        config: &BoardConfig,
        board: &BoardRef,
        ttl: Duration,
    ) -> Result<(Vec<Job>, bool), FetchError> {
        let endpoint = config.endpoint.as_ref().ok_or(FetchError::BadConfig)?;
        let got = self.get_board(db, endpoint, ttl, config.spacing()).await?;
        let jobs = rss::items(&got.body)
            .iter()
            .filter_map(|item| mapping::job_of(item, config, board))
            .collect();
        Ok((jobs, got.cached))
    }

    /// JSON-LD postings through a sitemap, on paths its robots rules (as
    /// checked) allow.
    async fn sitemap(
        &self,
        db: &PgPool,
        config: &BoardConfig,
        row: &BoardRow,
        board: &BoardRef,
        ttl: Duration,
    ) -> Result<(Vec<Job>, bool), FetchError> {
        let sitemap = config.sitemap.as_ref().ok_or(FetchError::BadConfig)?;
        let got = self.get_board(db, sitemap, ttl, config.spacing()).await?;
        let mut cached = got.cached;
        let most = usize::try_from(
            config
                .max_jobs
                .unwrap_or(20)
                .min(job_boards::MAX_SITEMAP_JOBS),
        )
        .unwrap_or(20);
        let rules = row.report().map(|r| r.robots.rules).unwrap_or_default();
        let fields = jsonld::config_with_fields(config);
        let mut jobs = Vec::new();
        for page in jsonld::sitemap_urls(
            &String::from_utf8_lossy(&got.body),
            config.job_path.as_deref(),
            most,
        ) {
            if !robots::allowed(&rules, &robots::path_of(&page)).0 {
                continue;
            }
            let got = match self.get_board(db, &page, ttl, config.spacing()).await {
                Ok(got) => got,
                Err(FetchError::Status(404)) => continue,
                Err(e) => return Err(e),
            };
            cached &= got.cached;
            for posting in jsonld::postings(&String::from_utf8_lossy(&got.body), &page) {
                if let Some(job) = mapping::job_of(&posting, &fields, board) {
                    jobs.push(job);
                }
            }
        }
        Ok((jobs, cached))
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
                detail.and_then(|detail| greenhouse::job_of_detail(detail, &mut board))
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
            Source::Arbeitnow | Source::Hn | Source::Board => None,
        };
        Ok(job.map(|job| (job, board)))
    }
}

/// `jobs` as their board's average salary needs them.
pub fn listed(jobs: &[Job]) -> Vec<ListedJob> {
    jobs.iter()
        .map(|job| {
            let board = job.board.as_ref();
            ListedJob {
                source_ref: job.source_ref.clone(),
                title: job.title.clone(),
                regions: board.map(|b| b.regions.clone()).unwrap_or_default(),
                engineering: pay::is_engineering(&job.title),
                salary_eur_year: board
                    .and_then(|b| b.pay.as_ref())
                    .and_then(pay::eur_per_year),
                posted_at: job.posted_at,
            }
        })
        .collect()
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
