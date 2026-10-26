//! What every handler and job can reach.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    Config,
    clock::{Clock, SystemClock},
    crypto::SecretKey,
    events::EventBus,
};

/// Shared application state. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    /// The database pool.
    pub db: PgPool,
    /// The configuration the server started with.
    pub config: Arc<Config>,
    /// The instance secret (sealing stored secrets).
    pub secret: SecretKey,
    /// Publishes domain events with their data change.
    pub events: EventBus,
    /// The time (a fixed clock in tests).
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    /// State over `db` with `config` and `secret`, the system clock and an
    /// event bus that knows no consumers yet.
    pub fn new(db: PgPool, config: Config, secret: SecretKey) -> Self {
        Self {
            db,
            config: Arc::new(config),
            secret,
            events: EventBus::default(),
            clock: Arc::new(SystemClock),
        }
    }

    /// This state, publishing through `events`.
    #[must_use]
    pub fn with_events(mut self, events: EventBus) -> Self {
        self.events = events;
        self
    }

    /// This state, telling time with `clock`.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
}
