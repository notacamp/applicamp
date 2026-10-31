//! What the pipeline needs from other modules, as ports the server wires
//! (modules never import each other): reading a job-board posting from a
//! link (sources), how far a person is (profile), their latest search
//! (sources), a company's own pages (sources), the profile as a job is
//! read against it, and facts from an answer (profile).

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
    /// How strong the profile is, 0 to 100.
    pub strength: u8,
    /// Where to go to make it stronger: `improve`, `skills`, `import` or
    /// `goals`; none once it is strong.
    pub next_action: Option<String>,
    /// Open questions of the profile review, when there is one.
    pub questions_left: Option<u32>,
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

/// One page of a company's site, as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePage {
    /// Where it was read.
    pub url: String,
    /// When.
    pub fetched_at: DateTime<Utc>,
    /// `posting`, `home` or `linked`.
    pub kind: String,
    /// Its text.
    pub text: String,
}

/// A company's site, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteReading {
    /// The pages, the posting first.
    pub pages: Vec<SitePage>,
    /// Pages left alone because the site's robots.txt forbids us.
    pub blocked: u32,
}

/// Why a site could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteFailure {
    /// Its address may not be asked, or it did not answer.
    Unreachable,
    /// Its robots.txt forbids its home page.
    RobotsDisallows,
    /// It answered, but no page had text.
    NothingReadable,
}

/// One position of the profile, as a job is read against it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleDigest {
    /// Its place in the profile's `work` list.
    pub index: usize,
    /// The title.
    pub position: Option<String>,
    /// The company.
    pub company: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    pub start_date: Option<String>,
    /// As `start_date`; none for a current position.
    pub end_date: Option<String>,
    /// The facts the person kept on it.
    pub facts: Vec<String>,
}

/// The profile in the fewest words: roles with dates and kept facts, and
/// skill names. No name, no contact details.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDigest {
    /// The positions.
    pub roles: Vec<RoleDigest>,
    /// The skills' names.
    pub skills: Vec<String>,
    /// The most recent position (its `index`).
    pub latest: Option<usize>,
    /// Skills the person said are not theirs (lower case).
    pub dismissed_skills: Vec<String>,
}

/// A fact taken from an answer, waiting for Keep, Fix or Drop (Grill me's
/// endpoints).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedFact {
    /// Its id (for `/api/v1/profile/grill/proposals/{id}/keep`).
    pub id: Uuid,
    /// The fact.
    pub text: String,
    /// The person's exact words.
    pub quote: String,
    /// What it is about (Grill me's areas).
    pub area: String,
    /// When it is deleted if nobody keeps it.
    pub expires_at: DateTime<Utc>,
}

/// What an answer about a job came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerFacts {
    /// The position the facts are about.
    pub position: usize,
    /// Its title.
    pub title: Option<String>,
    /// Its company.
    pub company: Option<String>,
    /// The facts, checked.
    pub facts: Vec<ProposedFact>,
    /// Lines the checks left out.
    pub dropped: u32,
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

    /// Whether `site` may be asked at all (a public https address; any
    /// http(s) one on an instance that allows private hosts).
    fn site_allowed(&self, site: &Url) -> bool;

    /// The home page of `site` and the pages it links to about the company,
    /// and `posting` (the job's own page), where robots.txt allows: never
    /// another host.
    fn read_site<'a>(
        &'a self,
        site: &'a Url,
        posting: Option<&'a Url>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, std::result::Result<SiteReading, SiteFailure>>;

    /// `account`'s profile as a job is read against it; none until there is
    /// a profile.
    fn profile_digest<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<ProfileDigest>>>;

    /// Read `answer` to `question` as facts about position `position` of
    /// the profile, as Grill me does: one small model call, the same
    /// checks, the facts waiting for Keep, Fix or Drop.
    #[allow(clippy::too_many_arguments)]
    fn answer_about<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        now: DateTime<Utc>,
        position: usize,
        question: &'a str,
        answer: &'a str,
        locale: Option<&'a str>,
    ) -> BoxFuture<'a, Result<AnswerFacts>>;

    /// Say `skill` is not one of the person's: not suggested or asked about
    /// again.
    fn dismiss_skill<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        skill: &'a str,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<()>>;
}
