//! What a search needs from other modules, as ports the server wires
//! (modules never import each other): what to match against (profile),
//! what is already in the pipeline and adding what it found (pipeline).

use std::collections::HashSet;

use applicamp_kernel::{BoxFuture, EventBus, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{job::Job, matching::MatchInput, scoring::Score};

/// A job a search found and scored.
#[derive(Debug, Clone)]
pub struct Scored {
    /// The job (with what the model read, for a free-text post).
    pub job: Job,
    /// Its score.
    pub score: Score,
}

/// The other modules, as a search sees them.
pub trait Neighbours: Send + Sync + 'static {
    /// What a search for `account` matches against; `None` until they have
    /// a profile and have answered a goals question.
    fn matching_input<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Option<MatchInput>>>;

    /// The accounts a nightly search runs for.
    fn accounts_with_goals<'a>(&'a self, db: &'a PgPool) -> BoxFuture<'a, Result<Vec<Uuid>>>;

    /// The jobs already in `account`'s pipeline (dismissed ones too), as
    /// `(source, source_ref)`.
    fn known_jobs<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<HashSet<(String, String)>>>;

    /// Add `found` to `account`'s pipeline as Found; how many were new.
    fn add_found<'a>(
        &'a self,
        db: &'a PgPool,
        events: &'a EventBus,
        account: Uuid,
        found: Vec<Scored>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>>;
}
