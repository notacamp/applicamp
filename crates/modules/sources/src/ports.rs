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
    /// Set when the job was read on a watched company's own job board
    /// (#20): the pipeline credits that board as its origin.
    pub own_board: Option<OwnBoard>,
}

/// A watched company's own job board, where a job was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnBoard {
    /// The company's name.
    pub company: String,
}

/// The slug under which the pipeline credits a watched company's own job
/// board on an opportunity (`pipeline.opportunity_sources.board`).
pub const OWN_BOARD: &str = "company-board";

/// A match of a person's, as the company watchers read it (the pipeline's
/// record of it).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchedPosting {
    /// The opportunity.
    pub opportunity: Uuid,
    /// The company, as the posting names it.
    pub company: String,
    /// The company's own website, when it is on record (an origin).
    pub site: Option<String>,
    /// The role.
    pub title: String,
    /// Its link, and the apply link, to tell when it came from a Greenhouse,
    /// Lever or Ashby board.
    pub url: String,
    pub apply_url: Option<String>,
    /// 0 to 100.
    pub match_score: u8,
    /// The person shortlisted it (or went further).
    pub shortlisted: bool,
    /// When it was found.
    pub found_at: DateTime<Utc>,
    /// `remote`, `hybrid` or `onsite`, when the posting says.
    pub work_mode: Option<String>,
    /// Where, as the posting writes it.
    pub location: Option<String>,
    /// Where it is open, as the posting says.
    pub region_note: Option<String>,
    /// Pay, as the posting writes it.
    pub salary_text: Option<String>,
    /// The start of its description, as plain text.
    pub description: String,
}

/// A fact read on a company's own site for a tailored CV (#34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyFact {
    /// The fact's id.
    pub id: Uuid,
    /// The company, as the opportunity names it.
    pub company: String,
    /// The fact.
    pub text: String,
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

/// An opportunity a search found whose reasons were never checked against
/// the profile (found before #27).
#[derive(Debug, Clone)]
pub struct Unreasoned {
    /// Whose.
    pub account: Uuid,
    /// The opportunity.
    pub opportunity: Uuid,
    /// The reasons it has now.
    pub why: Vec<String>,
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

    /// Up to `most` Found or Shortlisted opportunities a search found whose
    /// reasons were never checked, with an id after `after`, by id.
    fn unreasoned<'a>(
        &'a self,
        db: &'a PgPool,
        after: Uuid,
        most: usize,
    ) -> BoxFuture<'a, Result<Vec<Unreasoned>>>;

    /// `account`'s matches found since `since`: found by a search, not hidden
    /// by a requirement, not dismissed; with the company's site when it is on
    /// record. Oldest first.
    fn matched_postings<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        since: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Vec<MatchedPosting>>>;

    /// The facts read on these companies' own sites for `account` (kept, not
    /// removed), by company name.
    fn company_facts<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        companies: Vec<String>,
    ) -> BoxFuture<'a, Result<Vec<CompanyFact>>>;

    /// Keep `why` on `account`'s `opportunity` and mark its reasons checked
    /// (`updated`: some were stripped); only while it is still Found or
    /// Shortlisted and unchecked. Nothing else changes; never hidden or
    /// deleted. Whether it was.
    fn set_reasons<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        opportunity: Uuid,
        why: Vec<String>,
        updated: bool,
    ) -> BoxFuture<'a, Result<bool>>;
}
