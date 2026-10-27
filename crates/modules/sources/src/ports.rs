//! What a search needs from other modules, as ports the server wires
//! (modules never import each other): what to match against (profile),
//! what is already in the pipeline, adding what it found (crediting every
//! board it was seen on), and checking the requirements of what it found
//! before the rules (pipeline); and who may manage the job boards
//! (identity).

use std::collections::HashSet;

use applicamp_kernel::{BoxFuture, EventBus, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    job::{Job, SourceLink},
    matching::MatchInput,
    requirements::{Excluded, Facts},
    scoring::Score,
};

/// A job a search found and scored.
#[derive(Debug, Clone)]
pub struct Scored {
    /// The job (with what the model read, for a free-text post).
    pub job: Job,
    /// Its score.
    pub score: Score,
}

/// An opportunity a search found before the requirement rules: its
/// requirements were never checked.
#[derive(Debug, Clone)]
pub struct Unchecked {
    /// Whose.
    pub account: Uuid,
    /// The opportunity.
    pub opportunity: Uuid,
    /// The posting, as the pipeline keeps it.
    pub job: Job,
}

/// A job already in a person's pipeline under another board: the boards
/// it was seen on now, to add to its sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attach {
    /// Its apply link, compared without tracking ([`crate::dedupe`]).
    pub canonical_url: Option<String>,
    /// Its company, role and place.
    pub dedupe_key: Option<String>,
    /// The boards, each with the posting's link there.
    pub links: Vec<SourceLink>,
}

/// What a person's pipeline holds, by link and by company, role and place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownLinks {
    /// Canonical apply links.
    pub urls: HashSet<String>,
    /// Company, role and place, of opportunities added in the last 30 days.
    pub keys: HashSet<String>,
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

    /// The links and the company, role and place keys of `account`'s
    /// opportunities (keys only of those added since `since`).
    fn known_links<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        since: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<KnownLinks>>;

    /// Add the boards of `items` to the sources of `account`'s opportunity
    /// each one matches (by link first, then by key); how many matched.
    fn attach_sources<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        items: Vec<Attach>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>>;

    /// `account`'s address, to tell whether they manage the job boards.
    fn account_email<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<String>>>;

    /// Add `found` to `account`'s pipeline as Found; how many were new.
    fn add_found<'a>(
        &'a self,
        db: &'a PgPool,
        events: &'a EventBus,
        account: Uuid,
        found: Vec<Scored>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>>;

    /// Up to `most` Found or Shortlisted opportunities, found by a search,
    /// whose requirements were never checked; newest first.
    fn unchecked<'a>(
        &'a self,
        db: &'a PgPool,
        most: usize,
    ) -> BoxFuture<'a, Result<Vec<Unchecked>>>;

    /// Keep `facts` on `account`'s `opportunity`, hidden for `hidden` when
    /// it fails a requirement; only while it is still Found or Shortlisted
    /// and unchecked (never deleted, nothing else changed). Whether it was.
    fn set_requirements<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        opportunity: Uuid,
        facts: Facts,
        hidden: Option<Excluded>,
    ) -> BoxFuture<'a, Result<bool>>;
}
