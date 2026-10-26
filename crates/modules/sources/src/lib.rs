//! `applicamp-sources`: where jobs come from, and the search.
//!
//! - **Readers** ([`readers`]): Greenhouse, Lever and Ashby public job
//!   boards (the seed list `boards.toml`, plus boards people add by
//!   pasting a posting's link), Arbeitnow, and HN "Who is hiring". Public
//!   APIs only, never LinkedIn or Indeed (ADR 0006); one client with a
//!   timeout, a body cap, a per-host pace and a cache.
//! - **The search** ([`features::search`]): the goals' hard limits, a
//!   pre-rank without the model, the `complete` model's scores in batches
//!   of ten (cached per profile and goals version), and the good matches
//!   added to the pipeline as Found. "Search now" and a nightly search,
//!   both within the spending limits of `applicamp-ai`.
//!
//! What it needs from other modules (the profile and goals, the pipeline)
//! are [`ports::Neighbours`], which the server implements; modules never
//! import each other.
//!
//! Publishes `sources.search_finished` (ids and counts).

#![forbid(unsafe_code)]

pub mod boards;
pub mod config;
pub mod events;
pub mod features;
pub mod job;
pub mod lisbon;
pub mod matching;
pub mod ports;
pub mod readers;
pub mod scoring;

use std::sync::Arc;

use applicamp_ai::Gateway;
use applicamp_kernel::{
    Module,
    clock::{Clock, SystemClock},
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
}

impl Default for SourcesDeps {
    fn default() -> Self {
        Self {
            config: Arc::new(SourcesConfig::default()),
            reader: None,
            ai: None,
            neighbours: None,
            clock: Arc::new(SystemClock),
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
        })
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
    use features::search;
    let router = OpenApiRouter::new()
        .routes(routes!(search::start))
        .routes(routes!(search::latest))
        .routes(routes!(search::estimate))
        .layer(Extension(deps.clone()));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job_with::<search::RunSearch, _>(deps.clone())
            .job_with::<search::Nightly, _>(deps)
            .publishes::<events::SearchFinished>(),
    }
}

/// On server start: make `sources.boards` hold the seed list, and schedule
/// the nightly search (when it is on).
pub async fn start(db: &PgPool, deps: &SourcesDeps) -> Result<(), String> {
    let seed = boards::seed(&deps.config)?;
    let count = boards::sync(db, &seed).await.map_err(|e| e.to_string())?;
    let scheduled = features::search::schedule_nightly(db, deps)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        boards = count,
        nightly = deps.config.nightly(),
        scheduled,
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
        assert_eq!(registry.kinds(), ["sources.nightly", "sources.search"]);
    }
}
