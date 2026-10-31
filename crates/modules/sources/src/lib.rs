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
//! - **Discovery** ([`features::discovery`]): every week, new boards from a
//!   CC0 list are checked automatically (robots rules, terms sentences, a
//!   feed, a sample) and wait on the Boards page ([`features::boards`]),
//!   ranked by average salary ([`pay`]); nothing is read for jobs before
//!   approval. Approved boards are re-checked every 6 months.
//! - **Company boards** ([`readers`]): Greenhouse, Lever and Ashby public
//!   job boards (the seed list `boards.toml`, plus boards people add by
//!   pasting a posting's link); off in searches by default
//!   (`APPLICAMP_SOURCES__COMPANY_BOARDS`), kept for company watchers
//!   (#20).
//! - **The search** ([`features::search`]): the same job seen on several
//!   boards merged into one ([`dedupe`]), crediting each; the goals' hard
//!   limits on the boards' structured fields (a board's regions too:
//!   [`regions`]), a pre-rank without the model, the
//!   `complete` model's scores and eligibility facts in batches of ten
//!   (cached per profile, goals and rules version), the requirement rules
//!   ([`requirements`]) deciding pass or fail on those facts, and the good
//!   matches that pass added to the pipeline as Found. "Search now" and a
//!   nightly search, both within the spending limits of `applicamp-ai`.
//! - **The recheck** ([`features::recheck`]): at start, Found and
//!   Shortlisted opportunities found before the rules get their facts once,
//!   and the ones that fail are hidden (never deleted).
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
pub mod config;
pub mod dedupe;
pub mod events;
pub mod features;
pub mod grounding;
pub mod job;
pub mod job_boards;
pub mod lisbon;
pub mod matching;
pub mod net;
pub mod pages;
pub mod pay;
pub mod places;
pub mod ports;
pub mod readers;
pub mod regions;
pub mod requirements;
pub mod robots;
pub mod scans;
pub mod scoring;

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

pub use config::SourcesConfig;
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
    use features::{boards as board, discovery, recheck, reground, search};
    let router = OpenApiRouter::new()
        .routes(routes!(search::start))
        .routes(routes!(search::latest))
        .routes(routes!(search::estimate))
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
            .job_with::<discovery::CheckSeeded, _>(deps)
            .publishes::<events::SearchFinished>(),
        kinds: Vec::new(),
    }
}

/// On server start: make `sources.boards` hold the company boards' seed
/// list and the registry the job boards' (new boards added, the owner's
/// decisions kept), check the seeded boards waiting for the owner that were
/// never checked with their config, schedule the nightly search and the
/// weekly discovery (when they are on), and check the requirements of
/// opportunities found before the rules (once each).
pub async fn start(db: &PgPool, deps: &SourcesDeps) -> Result<(), String> {
    let seed = boards::seed(&deps.config)?;
    let count = boards::sync(db, &seed).await.map_err(|e| e.to_string())?;
    let job_seed = job_boards::seed(&deps.config)?;
    let job_boards = job_boards::sync(db, &job_seed, deps.clock.now())
        .await
        .map_err(|e| e.to_string())?;
    let discovery = features::discovery::schedule(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    let seeded_checks = features::discovery::schedule_seeded(db, deps)
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
                "sources.check_seeded_boards",
                "sources.discover_boards",
                "sources.nightly",
                "sources.recheck_requirements",
                "sources.reground_reasons",
                "sources.search"
            ]
        );
    }
}
