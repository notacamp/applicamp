//! `applicamp-sources`: where jobs come from, and the search.
//!
//! - **Job boards** ([`job_boards`]): a registry of boards, each read
//!   through its config by one of three generic adapters (a JSON API with a
//!   field mapping, an RSS or Atom feed, JSON-LD postings through a
//!   sitemap) or HN's extractor, with its terms honoured (polling interval,
//!   pace, attribution, unchanged apply links, expiry). Searches read only
//!   the boards the owner **approved** (Himalayas, Jobicy, Remote OK,
//!   Arbeitnow and HN first); remote jobs anywhere, open to where the
//!   person lives. Public APIs and feeds only, never LinkedIn or Indeed
//!   (ADR 0006).
//! - **Discovery** ([`features::discovery`]): every day, new boards from a
//!   CC0 list are checked automatically (robots rules, terms sentences, a
//!   feed, a sample) and wait on the Boards page ([`features::boards`]),
//!   ranked by average salary ([`pay`]); nothing is read for jobs before
//!   approval. Approved boards are re-checked every 6 months.
//! - **Company boards** ([`readers`]): Greenhouse, Lever and Ashby public
//!   job boards (the seed list `boards.toml`, plus boards people add by
//!   pasting a posting's link); off in searches by default
//!   (`APPLICAMP_SOURCES__COMPANY_BOARDS`), kept for company watchers
//!   (#20).
//! - **Company pages** ([`company_pages`], [`hiring_systems`],
//!   [`features::company_pages`], #87): the hiring systems companies publish
//!   their jobs on (Greenhouse, Lever, Ashby, Workable, Recruitee,
//!   Teamtailor, Personio) are rows of the registry, checked once each
//!   (robots rules, terms, content signal); every apply link of a job read
//!   names a company page, registered with no request, and a daily check
//!   approves it after one sample read when its system is approved and it
//!   posted lately. Searches do not read them yet (a job store comes first).
//! - **The Workable list** ([`features::workable_list`], #88): once a day
//!   Workable's documented list of every company's jobs is streamed (never
//!   held), and each company with a job posted in the last 60 days becomes a
//!   Workable company page of the registry, approved by the app with its
//!   system. No model; the jobs themselves are not kept.
//! - **The search** ([`features::search`]): the boards asked for the
//!   person's roles and top skills ([`matching::board_queries`]); the same
//!   job seen on several boards merged into one ([`dedupe`]), crediting
//!   each; the goals' hard limits on the boards' structured fields (a
//!   board's regions too: [`regions`]); a quick look at every job left
//!   ([`quick`], 25 a `complete` call) when there are more than one search
//!   reads in full; the best read in full, with their eligibility facts, in
//!   batches of ten ([`scoring`]; both cached per profile, goals and rules
//!   version); the requirement rules ([`requirements`]) deciding pass or
//!   fail on those facts, and the good matches that pass added to the
//!   pipeline as Found. "Search now" and a nightly search, both within the
//!   spending limits of `applicamp-ai`; what each did with every job read
//!   is counted for the Searches tab.
//! - **Company watchers** ([`features::watch`], #20): companies whose jobs
//!   keep fitting the person are suggested from their matches (no model),
//!   the person says Watch, the company's own job board is found (a posting's
//!   own link, else its site, robots.txt respected), and every day, inside
//!   the search, it is read and only the new postings that pass their hard
//!   limits are scored; a match lands in the pipeline as Found, credited to
//!   the company's own board.
//! - **The recheck** ([`features::recheck`]): at start, Found and
//!   Shortlisted opportunities found before the rules get their facts once,
//!   and the ones that fail are hidden (never deleted); the ones hidden only
//!   because their posting did not say where it hires are shown again, with
//!   that note.
//! - **The reason check** ([`grounding`], [`features::reground`]): a match
//!   reason may say only what the profile and goals say; the ones that
//!   claim more are dropped. At start, Found and Shortlisted opportunities
//!   found before the check get their reasons checked once (reasons
//!   stripped, never hidden or deleted).
//!
//! What it needs from other modules (the profile and goals, the pipeline)
//! are [`ports::Neighbours`], which the server implements; modules never
//! import each other.
//!
//! - **Scanned** ([`scans`]): what each search and discovery run scanned on
//!   each board, kept 90 days.
//!
//! Publishes `sources.search_finished` (ids and counts).

#![forbid(unsafe_code)]

pub mod board_keys;
pub mod boards;
pub mod company_pages;
pub mod config;
pub mod dedupe;
pub mod events;
pub mod features;
pub mod grounding;
pub mod hiring_systems;
pub mod job;
pub mod job_boards;
pub mod lisbon;
pub mod matching;
pub mod net;
pub mod pages;
pub mod pay;
pub mod places;
pub mod ports;
pub mod quick;
pub mod readers;
pub mod regions;
pub mod requirements;
pub mod robots;
pub mod scans;
pub mod scoring;
pub mod terms;

use std::sync::Arc;

use applicamp_ai::Gateway;
use applicamp_kernel::{
    Module,
    clock::{Clock, SystemClock},
    crypto::{Cipher, SecretKey},
    jobs::Background,
};
use axum::Extension;
use sqlx::PgPool;
use utoipa_axum::{router::OpenApiRouter, routes};

pub use config::{SearchConfig, SourcesConfig};
pub use features::search::{SearchRow, latest_row, read_posting};
pub use ports::Neighbours;

/// The module's name, schema and operation prefix.
pub const NAME: &str = "sources";

/// What the module is composed with.
#[derive(Clone)]
pub struct SourcesDeps {
    /// The settings.
    pub config: Arc<SourcesConfig>,
    /// The boards' reader; none in a catalogue composition.
    pub reader: Option<readers::Reader>,
    /// The AI gateway (its `complete` role scores jobs).
    pub ai: Option<Gateway>,
    /// The other modules, through the server.
    pub neighbours: Option<Arc<dyn Neighbours>>,
    /// The time.
    pub clock: Arc<dyn Clock>,
    /// Seals and opens the boards' API keys ([`board_keys`]); none until
    /// composed with the instance secret.
    pub keys: Option<Arc<Cipher>>,
}

impl Default for SourcesDeps {
    fn default() -> Self {
        Self {
            config: Arc::new(SourcesConfig::default()),
            reader: None,
            ai: None,
            neighbours: None,
            clock: Arc::new(SystemClock),
            keys: None,
        }
    }
}

impl std::fmt::Debug for SourcesDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcesDeps")
            .field("config", &self.config)
            .field("reader", &self.reader.is_some())
            .field("ai", &self.ai.is_some())
            .field("neighbours", &self.neighbours.is_some())
            .field("keys", &self.keys.is_some())
            .finish_non_exhaustive()
    }
}

impl SourcesDeps {
    /// Composed over `config`, `ai` and `neighbours`, on the system clock.
    pub fn new(
        config: SourcesConfig,
        ai: Option<Gateway>,
        neighbours: Arc<dyn Neighbours>,
    ) -> Result<Self, String> {
        let config = Arc::new(config);
        Ok(Self {
            reader: Some(readers::Reader::new(Arc::clone(&config))?),
            config,
            ai,
            neighbours: Some(neighbours),
            clock: Arc::new(SystemClock),
            keys: None,
        })
    }

    /// The same, sealing and opening the boards' API keys with the instance
    /// `secret` (the reader sends them to their boards).
    #[must_use]
    pub fn with_secret(mut self, secret: &SecretKey) -> Self {
        let keys = Arc::new(board_keys::cipher(secret));
        self.reader = self.reader.map(|r| r.with_keys(Arc::clone(&keys)));
        self.keys = Some(keys);
        self
    }

    /// The same, telling time with `clock`.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
}

/// The sources module, composed with `deps`.
pub fn module(deps: SourcesDeps) -> Module {
    use features::{
        board_requests as requests, boards as board, company_pages, discovery, recheck, reground,
        search, watch, workable_list,
    };
    let router = OpenApiRouter::new()
        .routes(routes!(search::start))
        .routes(routes!(search::latest))
        .routes(routes!(search::estimate))
        .routes(routes!(requests::overview))
        .routes(routes!(requests::list))
        .routes(routes!(requests::draft))
        .routes(routes!(requests::asked))
        .routes(routes!(requests::answer))
        .routes(routes!(requests::reopen))
        .routes(routes!(board::list))
        .routes(routes!(board::scanned))
        .routes(routes!(board::get))
        .routes(routes!(board::approve))
        .routes(routes!(board::decline))
        .routes(routes!(board::pause))
        .routes(routes!(board::set_config))
        .routes(routes!(board::check))
        .routes(routes!(board::set_key, board::forget_key))
        .routes(routes!(board::suggest))
        .routes(routes!(watch::list, watch::add))
        .routes(routes!(watch::watch))
        .routes(routes!(watch::not_interested))
        .routes(routes!(watch::stop))
        .routes(routes!(watch::careers))
        .layer(Extension(deps.clone()));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job_with::<search::RunSearch, _>(deps.clone())
            .job_with::<search::Nightly, _>(deps.clone())
            .job_with::<recheck::Recheck, _>(deps.clone())
            .job_with::<reground::Reground, _>(deps.clone())
            .job_with::<discovery::Discover, _>(deps.clone())
            .job_with::<discovery::CheckBoard, _>(deps.clone())
            .job_with::<discovery::CheckSeeded, _>(deps.clone())
            .job_with::<company_pages::CheckPages, _>(deps.clone())
            .job_with::<company_pages::CheckPagesSoon, _>(deps.clone())
            .job_with::<company_pages::web_search::WebSearch, _>(deps.clone())
            .job_with::<workable_list::ReadList, _>(deps.clone())
            .job_with::<workable_list::ReadListSoon, _>(deps.clone())
            .job_with::<watch::FindBoard, _>(deps)
            .publishes::<events::SearchFinished>(),
        kinds: Vec::new(),
    }
}

/// On server start: make `sources.boards` hold the company boards' seed
/// list and the registry the job boards' (new boards added, the owner's
/// decisions kept), check the seeded boards waiting for the owner that were
/// never checked with their config, schedule the nightly search and the
/// daily discovery (when they are on), and check the requirements of
/// opportunities found before the rules (once each).
pub async fn start(db: &PgPool, deps: &SourcesDeps) -> Result<(), String> {
    let seed = boards::seed(&deps.config)?;
    let count = boards::sync(db, &seed).await.map_err(|e| e.to_string())?;
    let job_seed = job_boards::seed(&deps.config)?;
    let job_boards = job_boards::sync(db, &job_seed, deps.clock.now())
        .await
        .map_err(|e| e.to_string())?;
    let now = deps.clock.now();
    company_pages::ensure_systems(db, now)
        .await
        .map_err(|e| e.to_string())?;
    let company_pages_added =
        company_pages::register_boards(db, &boards::all(db).await.map_err(|e| e.to_string())?, now)
            .await
            .map_err(|e| e.to_string())?;
    let web_search = features::company_pages::web_search::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let workable_list = features::workable_list::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let discovery = features::discovery::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let seeded_checks = features::discovery::schedule_seeded(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    // After the seeded boards' check, which may name company pages: the
    // first reads of the day then wait behind it.
    let page_checks = features::company_pages::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let scheduled = features::search::schedule_nightly(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let recheck = features::recheck::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let reground = features::reground::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        boards = count,
        job_boards,
        company_pages_added,
        page_checks,
        web_search,
        workable_list,
        company_boards = deps.config.company_boards(),
        discovery,
        seeded_checks,
        nightly = deps.config.nightly(),
        scheduled,
        recheck,
        reground,
        "job sources ready"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod testkit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_background_work_registers() {
        let registry = applicamp_kernel::jobs::Registry::build(vec![(
            NAME,
            module(SourcesDeps::default()).background,
        )])
        .unwrap();
        assert_eq!(
            registry.kinds(),
            [
                "sources.check_board",
                "sources.check_company_pages",
                "sources.check_company_pages_soon",
                "sources.check_seeded_boards",
                "sources.discover_boards",
                "sources.find_board",
                "sources.nightly",
                "sources.recheck_requirements",
                "sources.reground_reasons",
                "sources.search",
                "sources.web_search_pages",
                "sources.workable_list",
                "sources.workable_list_soon"
            ]
        );
    }
}
