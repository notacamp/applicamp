//! What the pipeline needs from other modules, as ports the server wires
//! (modules never import each other): reading a job-board posting from a
//! link (sources), how far a person is (profile), their latest search
//! (sources).

use applicamp_kernel::{BoxFuture, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

/// A posting read from a job board.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Posting {
    /// `greenhouse`, `lever` or `ashby`.
    pub source: String,
    /// Its id at the source.
    pub source_ref: String,
    /// The role.
    pub title: String,
    /// The company.
    pub company: String,
    /// Where, as the board writes it.
    pub location: Option<String>,
    /// `remote`, `hybrid` or `onsite`, when the board says.
    pub work_mode: Option<String>,
    /// The posting's page.
    pub url: String,
    /// When it was published.
    pub posted_at: Option<DateTime<Utc>>,
    /// Pay, as the board writes it.
    pub salary_text: Option<String>,
    /// The description, as plain text.
    pub description: Option<String>,
}

/// How far a person is with their profile and goals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileStatus {
    /// They have a profile with something in it.
    pub profile_done: bool,
    /// What would make it stronger: `add_headline`, `add_work`, `add_skills`.
    pub strength_hint: Option<String>,
    /// The profile lists no skills and its text names no technology.
    pub no_skills: bool,
    /// Goals questions answered.
    pub goals_answered: u32,
    /// Goals questions there are.
    pub goals_total: u32,
}

/// A person's latest search, for Today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLine {
    /// The search.
    pub id: Uuid,
    /// `queued`, `running`, `finished` or `failed`.
    pub status: String,
    /// `manual` or `nightly`.
    pub trigger: String,
    /// When it started.
    pub started_at: DateTime<Utc>,
    /// When it ended.
    pub finished_at: Option<DateTime<Utc>>,
    /// Jobs read.
    pub fetched: i32,
    /// Opportunities it added.
    pub added: i32,
    /// Why it stopped early, if it did.
    pub stopped: Option<String>,
}

/// The other modules, as the pipeline sees them.
pub trait Neighbours: Send + Sync + 'static {
    /// The posting at `url` when it is a Greenhouse, Lever or Ashby posting
    /// (read from the board's API, whose board is then searched for
    /// `account` too); `None` for any other link, or one that could not be
    /// read. Never fetches anything but those boards' APIs.
    fn read_posting<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        url: &'a Url,
    ) -> BoxFuture<'a, Result<Option<Posting>>>;

    /// How far `account` is.
    fn profile_status<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<ProfileStatus>>;

    /// `account`'s latest search, if any.
    fn latest_search<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<SearchLine>>>;
}
