//! The brief: the real facts a tailored CV is based on.
//!
//! For an opportunity (`/api/v1/opportunities/{id}/brief…`):
//!
//! - **The company's facts.** The company's own pages (its home page, a few
//!   it links to about the company, and the posting) are read on a click and
//!   become short facts, each with the words of the page that say it. The
//!   site is the board's, else the apply link's host, else one the posting
//!   names, else one found from the company's name, else the person's, typed
//!   once ([`site`]). Nothing but the posting and the company's own site is
//!   ever fetched, and only where robots.txt allows. A fact whose quote is not
//!   on the page it cites is left out ([`judge`]). The person can remove any
//!   fact; a removed fact stays removed. Cached 30 days per company.
//! - **What the job wants.** One more call compares the posting with the
//!   company's facts and a compact profile: up to six things the job wants
//!   (each quoting the posting) and up to three questions where the profile is
//!   thin.
//! - **Answers.** An answer to a question becomes facts on a position, through
//!   Grill me's own pipeline and checks, kept with its Keep, Fix and Drop.
//!   "I don't have it" on a skill question records a skill dismissal and
//!   makes no call.
//!
//! Every model call starts with a click, or with the preparation of a job the
//! person shortlisted ([`crate::features::prepare`], ADR 0010); nothing runs on
//! a timer. The AI budget reserves before each call, and a company read is
//! capped at [`DAILY_READS_CAP`] per account per UTC day (its fetches count,
//! whether or not the model was reached).

pub mod employer;
pub mod judge;
pub mod prompt;
pub mod site;
#[cfg(test)]
mod tests;

use applicamp_ai::{Caller, ChatRequest, Gateway, Message, ResponseFormat, Role};
use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use self::{
    judge::{Analysis, NewFact, RawAnalysis, RawFacts, fingerprint, squash},
    prompt::FactLine,
};
use crate::{
    PipelineDeps,
    companies::{self, Company, ReadPage, SiteSource},
    features::prepare::{self, Preparation},
    ports::{Neighbours, ProfileDigest, SiteFailure, SitePage},
    store::{MAX_DESCRIPTION, MAX_URL},
};

/// Company reads one account can start in a UTC day.
pub const DAILY_READS_CAP: i64 = 20;
/// How long a company's facts are reused before they are read again.
pub const FRESH_DAYS: i64 = 30;
/// About what reading a company costs, in euros: one call.
pub const COMPANY_READ_EUR: f64 = 0.002;
/// About what comparing a job costs, in euros: one call.
pub const ANALYSIS_EUR: f64 = 0.002;
/// About what answering a question costs, in euros: one small call.
pub const ANSWER_EUR: f64 = 0.0005;
/// Answer budgets in tokens: the first attempt, and the one retry after a
/// cut-off (a reasoning model thinks first, and thinking counts).
const BUDGETS: [u32; 2] = [4_000, 8_000];
/// The facts call's answer budget, both attempts (the second gets less to
/// read, not more room).
const FACTS_TOKENS: u32 = 4_000;
/// Longest answer, in characters (Grill me's).
const MAX_ANSWER: usize = 2_000;

// ---- views ----------------------------------------------------------------

/// Where the company's site came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SiteOrigin {
    /// The job board gave it.
    Board,
    /// The apply link's host.
    ApplyHost,
    /// The person typed it.
    Person,
    /// A link in the posting.
    PostingLink,
    /// An email address in the posting.
    PostingEmail,
    /// A domain made from the company's name, whose home page names it.
    NameGuess,
}

impl From<SiteSource> for SiteOrigin {
    fn from(source: SiteSource) -> Self {
        match source {
            SiteSource::Board => Self::Board,
            SiteSource::ApplyHost => Self::ApplyHost,
            SiteSource::Person => Self::Person,
            SiteSource::PostingLink => Self::PostingLink,
            SiteSource::PostingEmail => Self::PostingEmail,
            SiteSource::NameGuess => Self::NameGuess,
        }
    }
}

/// A page that was fetched.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BriefPage {
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    /// `posting`, `home` or `linked`.
    pub kind: String,
}

/// The last read of the company's pages.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompanyRead {
    /// When.
    pub read_at: DateTime<Utc>,
    /// The facts are reused until then; "Read again" reads sooner.
    pub fresh_until: DateTime<Utc>,
    /// The pages fetched.
    pub pages: Vec<BriefPage>,
    /// Pages left alone because the site's robots.txt forbids us.
    pub pages_blocked: u32,
    /// Facts left out because their quote is not on the page they cite.
    pub facts_dropped: u32,
}

/// A fact read on the company's own site.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BriefFact {
    pub id: Uuid,
    /// The fact.
    pub text: String,
    /// The page's own words that say it.
    pub quote: String,
    /// The page it was read on.
    pub url: String,
    /// When that page was fetched.
    pub fetched_at: Option<DateTime<Utc>>,
    /// The person removed it ("Wrong, remove"); Undo restores it.
    pub removed: bool,
}

/// Something the job wants, with the posting's words.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BriefWant {
    /// Short.
    pub label: String,
    /// The posting's exact words for it.
    pub quote: String,
    /// `must` when the posting requires it, `nice` when it only prefers it.
    pub level: String,
}

/// Where a question is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionState {
    /// Waiting for an answer.
    Open,
    /// Answered: its facts wait on the position (or were kept).
    Answered,
    /// Hidden for this opportunity.
    Skipped,
    /// "I don't have it": the skill is not asked about again.
    DontHave,
}

impl QuestionState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
            Self::Skipped => "skipped",
            Self::DontHave => "dont_have",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "answered" => Self::Answered,
            "skipped" => Self::Skipped,
            "dont_have" => Self::DontHave,
            _ => Self::Open,
        }
    }
}

/// A position of the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleRef {
    /// Its place in the profile's `work` list.
    pub position: u32,
    /// Its title.
    pub title: Option<String>,
    /// Its company.
    pub company: Option<String>,
}

/// A question where the profile is thin for what the job wants.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BriefQuestion {
    pub id: Uuid,
    /// It names the company fact or the posting line it comes from.
    pub question: String,
    /// The position an answer is about; none: the most recent one
    /// (`latest_role` of the brief) and the page says so.
    pub about: Option<RoleRef>,
    /// The skill it is about, if it is about one: "I don't have it" is
    /// offered.
    pub skill: Option<String>,
    pub state: QuestionState,
}

/// What each step costs, in euros, about.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct Estimates {
    /// Reading the company's pages: one call, reused for 30 days.
    pub company_read_eur: f64,
    /// Comparing the job: one call, reused until the posting changes.
    pub analysis_eur: f64,
    /// Answering one question: one small call.
    pub answer_eur: f64,
}

/// What was spent, and the cap on company reads.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct BriefSpend {
    pub today_eur: f64,
    pub day_limit_eur: Option<f64>,
    pub month_eur: f64,
    pub month_limit_eur: Option<f64>,
    /// Company reads started today (UTC).
    pub reads_today: u32,
    /// The most company reads an account can start in a UTC day.
    pub reads_cap: u32,
}

/// The brief of an opportunity.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Brief {
    pub opportunity_id: Uuid,
    /// The company, as the opportunity names it.
    pub company: String,
    /// Its website (an origin); none until it is known.
    pub site: Option<String>,
    pub site_source: Option<SiteOrigin>,
    /// The person has to type the site (once).
    pub site_needed: bool,
    /// The last read; none until the company was read.
    pub read: Option<CompanyRead>,
    /// The facts, removed ones included (marked), oldest first.
    pub facts: Vec<BriefFact>,
    /// What the job wants most; none until it was compared.
    pub wants: Option<Vec<BriefWant>>,
    /// The questions, in the order they are asked.
    pub questions: Vec<BriefQuestion>,
    /// The most recent position of the profile: where an answer goes when
    /// its question names none.
    pub latest_role: Option<RoleRef>,
    pub estimates: Estimates,
    pub spend: BriefSpend,
    /// What the app does by itself for this job: none until it started.
    pub preparation: Option<Preparation>,
    /// The app prepares shortlisted jobs by itself: the person's Autonomy goal
    /// is "Prepare" (or not answered) and a model is set up.
    pub prepares_by_itself: bool,
}

/// A fact taken from an answer, waiting for Keep, Fix or Drop.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AnswerProposal {
    /// For `POST /api/v1/profile/grill/proposals/{id}/keep` (and `…/drop`).
    pub id: Uuid,
    pub text: String,
    /// The person's exact words.
    pub quote: String,
    /// What it is about (Grill me's areas).
    pub area: String,
    pub expires_at: DateTime<Utc>,
}

/// What an answer came to.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AnswerResult {
    pub question: BriefQuestion,
    /// The position the facts are about.
    pub role: RoleRef,
    /// The question named no position, so the most recent one was used.
    pub defaulted: bool,
    /// The facts, checked against the person's words and the profile.
    pub proposals: Vec<AnswerProposal>,
    /// Lines the checks left out.
    pub dropped: u32,
}

/// The company's website, typed.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SiteRequest {
    /// An http or https address; only its host (and port) is kept.
    pub url: String,
}

/// The person's answer.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerRequest {
    /// In their own words, 1 to 2000 characters.
    pub text: String,
}

/// The language of the questions, and whether to read again.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReadQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
    /// Read the company's pages again, even though its facts are fresh.
    again: Option<bool>,
}

/// The language of the answer's facts.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LanguageQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
}

// ---- storage --------------------------------------------------------------

/// The columns of an opportunity the brief needs.
#[derive(sqlx::FromRow)]
struct Opp {
    id: Uuid,
    title: String,
    company: String,
    url: String,
    source: String,
    apply_url: Option<String>,
    description: Option<String>,
}

async fn load(db: &PgPool, account: Uuid, id: Uuid) -> Result<Opp> {
    sqlx::query_as(
        "SELECT id, title, company, url, source, apply_url, description
           FROM pipeline.opportunities WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)
}

/// The hosts of the job boards `opp` was seen on.
async fn board_hosts(db: &PgPool, opp: Uuid) -> Result<Vec<String>> {
    let urls: Vec<String> = sqlx::query_scalar(
        "SELECT url FROM pipeline.opportunity_sources WHERE opportunity_id = $1",
    )
    .bind(opp)
    .fetch_all(db)
    .await?;
    Ok(urls
        .iter()
        .filter_map(|u| Url::parse(u).ok())
        .filter_map(|u| u.host_str().map(str::to_owned))
        .collect())
}

fn neighbours(deps: &PipelineDeps) -> Result<&dyn Neighbours> {
    deps.neighbours
        .as_deref()
        .ok_or(Error::Unavailable("neighbours_unavailable"))
}

fn gateway(deps: &PipelineDeps) -> Result<&Gateway> {
    match &deps.ai {
        Some(gateway) if gateway.is_configured(Role::Complete) => Ok(gateway),
        _ => Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    }
}

async fn digest(deps: &PipelineDeps, db: &PgPool, account: Uuid) -> Result<Option<ProfileDigest>> {
    match &deps.neighbours {
        Some(neighbours) => neighbours.profile_digest(db, account).await,
        None => Ok(None),
    }
}

/// The site of `opp`'s company, as known now.
async fn site_of(db: &PgPool, account: Uuid, opp: &Opp) -> Result<(Option<Company>, site::Choice)> {
    let company = companies::find(db, account, &opp.company).await?;
    let boards = board_hosts(db, opp.id).await?;
    let choice = site::choose(
        company.as_ref(),
        opp.apply_url.as_deref(),
        &opp.url,
        opp.source == crate::store::LINK,
        &boards,
        opp.description.as_deref(),
        &opp.company,
    );
    Ok((company, choice))
}

/// Whether opportunity `id`'s job was compared (what it wants is stored).
pub(crate) async fn was_compared(db: &PgPool, id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pipeline.job_briefs WHERE opportunity_id = $1)",
    )
    .bind(id)
    .fetch_one(db)
    .await?)
}

/// Whether the site of opportunity `id`'s company is known now, without
/// looking for it.
pub(crate) async fn site_known(db: &PgPool, account: Uuid, id: Uuid) -> Result<bool> {
    let opp = load(db, account, id).await?;
    Ok(site_of(db, account, &opp).await?.1.site.is_some())
}

/// The company's site as [`site_of`] knows it, else found by asking at most
/// [`site::MOST_TRIED`] home pages ([`site::to_try`]): the first that names
/// the company is kept on it; when none does, it is not looked for again for
/// [`FRESH_DAYS`] days. No model call.
async fn find_site(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: &Opp,
    now: DateTime<Utc>,
) -> Result<site::Choice> {
    let (_, choice) = site_of(db, account, opp).await?;
    if choice.site.is_some() || opp.company.trim().is_empty() {
        return Ok(choice);
    }
    let since = now - TimeDelta::days(FRESH_DAYS);
    if companies::sought_since(db, account, &opp.company, since).await? {
        return Ok(choice);
    }
    let neighbours = neighbours(deps)?;
    let boards = board_hosts(db, opp.id).await?;
    for found in site::to_try(opp.description.as_deref(), &boards, &opp.company) {
        if !neighbours.site_allowed(&found.site) {
            continue;
        }
        let Ok(text) = neighbours.home_page(&found.site).await else {
            continue;
        };
        if site::names(&text, &opp.company) {
            let mut tx = db.begin().await?;
            companies::note_site(
                &mut tx,
                account,
                &opp.company,
                &found.site,
                found.source,
                now,
            )
            .await?;
            tx.commit().await?;
            // Ids and kinds only.
            tracing::info!(source = found.source.as_str(), "a company's site was found");
            return Ok(site::Choice {
                site: Some(found.site),
                source: Some(found.source),
            });
        }
    }
    companies::note_sought(db, account, &opp.company, now).await?;
    tracing::info!("a company's site was not found");
    Ok(choice)
}

fn role_ref(digest: Option<&ProfileDigest>, position: usize) -> Option<RoleRef> {
    let role = digest?.roles.iter().find(|r| r.index == position)?;
    Some(RoleRef {
        position: u32::try_from(role.index).ok()?,
        title: role.position.clone(),
        company: role.company.clone(),
    })
}

#[derive(sqlx::FromRow)]
struct QuestionRow {
    id: Uuid,
    question: String,
    about: Option<i32>,
    skill: Option<String>,
    state: String,
}

impl QuestionRow {
    fn view(self, digest: Option<&ProfileDigest>) -> BriefQuestion {
        BriefQuestion {
            id: self.id,
            question: self.question,
            about: self
                .about
                .and_then(|a| usize::try_from(a).ok())
                .and_then(|a| role_ref(digest, a)),
            skill: self.skill,
            state: QuestionState::parse(&self.state),
        }
    }
}

async fn reads_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.company_reads WHERE account_id = $1 AND started_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

fn counted(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

async fn spend_of(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<BriefSpend> {
    let spend = match &deps.ai {
        Some(ai) => ai.spend(db, account, now).await?,
        None => applicamp_ai::Spend::default(),
    };
    Ok(BriefSpend {
        today_eur: spend.today_eur,
        day_limit_eur: spend.day_limit_eur,
        month_eur: spend.month_eur,
        month_limit_eur: spend.month_limit_eur,
        reads_today: counted(reads_today(db, account, now).await?),
        reads_cap: counted(DAILY_READS_CAP),
    })
}

#[derive(sqlx::FromRow)]
struct FactRow {
    id: Uuid,
    text: String,
    quote: String,
    url: String,
    removed: bool,
    fetched_at: Option<DateTime<Utc>>,
}

/// The brief of `opp`, as it is now. Never calls the model.
async fn build(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: &Opp,
    now: DateTime<Utc>,
) -> Result<Brief> {
    let (company, choice) = site_of(db, account, opp).await?;
    let profile = digest(deps, db, account).await?;

    let mut facts = Vec::new();
    if let Some(company) = &company {
        let rows: Vec<FactRow> = sqlx::query_as(
            "SELECT f.id, f.text, f.quote, f.url, f.removed, p.fetched_at
               FROM pipeline.company_facts f
               LEFT JOIN pipeline.company_pages p
                 ON p.company_id = f.company_id AND p.url = f.url
              WHERE f.company_id = $1 ORDER BY f.created_at, f.id",
        )
        .bind(company.id)
        .fetch_all(db)
        .await?;
        facts = rows
            .into_iter()
            .map(|row| BriefFact {
                id: row.id,
                text: row.text,
                quote: row.quote,
                url: row.url,
                fetched_at: row.fetched_at,
                removed: row.removed,
            })
            .collect();
    }
    let read = company.as_ref().and_then(|c| {
        c.read_at.map(|read_at| CompanyRead {
            read_at,
            fresh_until: read_at + TimeDelta::days(FRESH_DAYS),
            pages: c
                .pages
                .0
                .iter()
                .map(|p| BriefPage {
                    url: p.url.clone(),
                    fetched_at: p.fetched_at,
                    kind: p.kind.clone(),
                })
                .collect(),
            pages_blocked: u32::try_from(c.pages_blocked).unwrap_or(0),
            facts_dropped: u32::try_from(c.facts_dropped).unwrap_or(0),
        })
    });

    let wants: Option<sqlx::types::Json<Vec<BriefWant>>> =
        sqlx::query_scalar("SELECT wants FROM pipeline.job_briefs WHERE opportunity_id = $1")
            .bind(opp.id)
            .fetch_optional(db)
            .await?;
    let dismissed: Vec<String> = profile
        .as_ref()
        .map(|p| p.dismissed_skills.clone())
        .unwrap_or_default();
    let questions: Vec<QuestionRow> = sqlx::query_as(
        "SELECT id, question, about, skill, state FROM pipeline.brief_questions
          WHERE opportunity_id = $1 ORDER BY seq",
    )
    .bind(opp.id)
    .fetch_all(db)
    .await?;
    let questions = questions
        .into_iter()
        .filter(|q| {
            // A skill the person said is not theirs, on any job, is not asked.
            q.state != "open"
                || q.skill
                    .as_deref()
                    .is_none_or(|s| !dismissed.contains(&s.to_lowercase()))
        })
        .map(|q| q.view(profile.as_ref()))
        .collect();

    Ok(Brief {
        opportunity_id: opp.id,
        company: opp.company.clone(),
        site_needed: choice.site.is_none() && !opp.company.trim().is_empty(),
        site: choice.site.map(|s| s.to_string()),
        site_source: choice.source.map(SiteOrigin::from),
        read,
        facts,
        wants: wants.map(|w| w.0),
        questions,
        latest_role: profile
            .as_ref()
            .and_then(|p| p.latest)
            .and_then(|l| role_ref(profile.as_ref(), l)),
        estimates: Estimates {
            company_read_eur: COMPANY_READ_EUR,
            analysis_eur: ANALYSIS_EUR,
            answer_eur: ANSWER_EUR,
        },
        spend: spend_of(deps, db, account, now).await?,
        preparation: prepare::view(db, opp.id).await?,
        prepares_by_itself: prepare::by_itself(deps, db, account).await?,
    })
}

/// The brief of opportunity `id`, as it is now. Never calls the model.
pub(crate) async fn brief_of(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<Brief> {
    let opp = load(db, account, id).await?;
    build(deps, db, account, &opp, now).await
}

/// What [`read_and_compare`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Prepared {
    /// The company's site is known (found now, or before).
    pub site: bool,
    /// Its pages were read just now (one model call).
    pub read: bool,
    /// The job was compared just now (one model call).
    pub compared: bool,
}

/// Read the company (unless its facts are fresh) and compare the job (unless
/// it was compared for this very posting text). The site is found first when
/// it is not known ([`find_site`]). Without a site, nothing more is done
/// unless `without_site`: then the job is compared without the company's
/// facts. A site that cannot be read (robots.txt, no text, no answer) does not
/// stop the comparison.
pub(crate) async fn read_and_compare(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    id: Uuid,
    locale: Option<&str>,
    without_site: bool,
    now: DateTime<Utc>,
) -> Result<Prepared> {
    let opp = load(db, account, id).await?;
    let mut prepared = Prepared::default();
    if !opp.company.trim().is_empty() {
        match read_company(deps, db, account, &opp, false, now).await {
            Ok(read) => {
                prepared.site = true;
                prepared.read = read;
            }
            Err(Error::BadRequest("site_needed", _)) => {}
            Err(e)
                if matches!(
                    e.code(),
                    "robots_disallows" | "no_pages_readable" | "site_unreachable"
                ) =>
            {
                prepared.site = true;
                tracing::info!(error = e.code(), "a company could not be read");
            }
            Err(e) => return Err(e),
        }
    }
    if !prepared.site && !without_site {
        return Ok(prepared);
    }
    prepared.compared = analyze(deps, db, account, id, locale, now).await?;
    Ok(prepared)
}

/// What a job's questions still wait on: how many are open (a skill the
/// person said is not theirs is not asked), and the facts the answers
/// proposed (some may still wait for Keep or Drop).
pub(crate) async fn questions_left(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    id: Uuid,
) -> Result<(u32, Vec<Uuid>)> {
    let dismissed: Vec<String> = digest(deps, db, account)
        .await?
        .map(|p| p.dismissed_skills)
        .unwrap_or_default();
    let rows: Vec<(String, Option<String>, Vec<Uuid>)> = sqlx::query_as(
        "SELECT state, skill, proposals FROM pipeline.brief_questions
          WHERE opportunity_id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account)
    .fetch_all(db)
    .await?;
    let open = rows
        .iter()
        .filter(|(state, skill, _)| {
            state == QuestionState::Open.as_str()
                && skill
                    .as_deref()
                    .is_none_or(|s| !dismissed.contains(&s.to_lowercase()))
        })
        .count();
    let proposals = rows.into_iter().flat_map(|(_, _, p)| p).collect();
    Ok((u32::try_from(open).unwrap_or(u32::MAX), proposals))
}

// ---- the model ------------------------------------------------------------

/// Ask `system` / `user` for a JSON object shaped like `schema`: one call,
/// a second only after a cut-off. A spending limit ends the attempt at once;
/// nothing retries past it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ask<T: DeserializeOwned>(
    gateway: &Gateway,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    name: &str,
    schema: &Value,
    system: &str,
    user: &str,
) -> Result<T> {
    ask_as(
        gateway,
        Role::Complete,
        &BUDGETS,
        db,
        account,
        now,
        name,
        schema,
        system,
        user,
    )
    .await
    .map(|(read, _)| read)
}

/// [`ask`] on `role`, trying each of `budgets` (`max_tokens`) in turn while
/// the answer is cut off or unusable. Returns the answer and what the calls
/// cost, in US dollars.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ask_as<T: DeserializeOwned>(
    gateway: &Gateway,
    role: Role,
    budgets: &[u32],
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    name: &str,
    schema: &Value,
    system: &str,
    user: &str,
) -> Result<(T, f64)> {
    let mut last = Error::Unavailable(applicamp_ai::AI_FAILED);
    let mut cost = 0.0;
    for &max_tokens in budgets {
        let (attempt, spent) = ask_attempt_as(
            gateway, role, db, account, now, name, schema, system, user, max_tokens,
        )
        .await?;
        cost += spent;
        match attempt {
            Attempt::Done(read) => return Ok((read, cost)),
            Attempt::Retry(error) => last = error,
        }
    }
    Err(last)
}

/// What one call came to.
pub(crate) enum Attempt<T> {
    /// The answer.
    Done(T),
    /// Cut off, or not usable: try again, with the error to give if it was
    /// the last try.
    Retry(Error),
}

/// One call with `max_tokens`. A spending limit is an `Err` at once.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ask_attempt<T: DeserializeOwned>(
    gateway: &Gateway,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    name: &str,
    schema: &Value,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<Attempt<T>> {
    ask_attempt_as(
        gateway,
        Role::Complete,
        db,
        account,
        now,
        name,
        schema,
        system,
        user,
        max_tokens,
    )
    .await
    .map(|(attempt, _)| attempt)
}

/// [`ask_attempt`] on `role`, with what the call cost in US dollars.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ask_attempt_as<T: DeserializeOwned>(
    gateway: &Gateway,
    role: Role,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    name: &str,
    schema: &Value,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<(Attempt<T>, f64)> {
    let caller = Caller { db, account, now };
    let request = ChatRequest {
        temperature: Some(0.0),
        max_tokens: Some(max_tokens),
        response_format: Some(ResponseFormat::json_schema(name, schema.clone())),
        ..ChatRequest::new(vec![Message::system(system), Message::user(user)])
    };
    let reply = gateway.complete_as(role, caller, request).await?;
    let cost = reply.cost_usd.unwrap_or_default();
    if reply.finish_reason.as_deref() == Some("length") {
        tracing::warn!(max_tokens, call = name, "an answer was cut off");
        return Ok((
            Attempt::Retry(Error::Unavailable(applicamp_ai::AI_FAILED)),
            cost,
        ));
    }
    match reply.json::<T>() {
        Ok(read) => Ok((Attempt::Done(read), cost)),
        Err(error) => {
            tracing::warn!(%error, call = name, "an answer was unusable");
            Ok((Attempt::Retry(Error::from(error)), cost))
        }
    }
}

// ---- reading --------------------------------------------------------------

/// Read the company's pages and make its facts (**one model call**), unless
/// they are fresh and `again` is not asked for. Returns whether it read.
#[allow(clippy::too_many_arguments)]
async fn read_company(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: &Opp,
    again: bool,
    now: DateTime<Utc>,
) -> Result<bool> {
    let choice = find_site(deps, db, account, opp, now).await?;
    let company = companies::find(db, account, &opp.company).await?;
    if !again
        && company
            .as_ref()
            .and_then(|c| c.read_at)
            .is_some_and(|at| now < at + TimeDelta::days(FRESH_DAYS))
    {
        return Ok(false);
    }
    let Some(site) = choice.site else {
        return Err(Error::BadRequest(
            "site_needed",
            "Type the company's website first.".to_owned(),
        ));
    };
    let source = choice.source.unwrap_or(SiteSource::Person);
    let posting = Url::parse(&opp.url).ok();
    fetch_and_store(
        deps,
        db,
        account,
        &opp.company,
        &site,
        source,
        posting.as_ref(),
        Some(opp),
        now,
    )
    .await?;
    Ok(true)
}

/// Read `company`'s pages at `site` and keep the facts (**one model call**):
/// the one mechanism behind a job's company and a past employer. The read
/// counts toward the daily cap whatever comes of it.
#[allow(clippy::too_many_arguments)]
async fn fetch_and_store(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    company: &str,
    site: &Url,
    source: SiteSource,
    posting: Option<&Url>,
    opp: Option<&Opp>,
    now: DateTime<Utc>,
) -> Result<()> {
    let neighbours = neighbours(deps)?;
    let gateway = gateway(deps)?;
    if reads_today(db, account, now).await? >= DAILY_READS_CAP {
        return Err(Error::TooManyRequests(
            "company_reads_daily_cap",
            format!("You can read {DAILY_READS_CAP} companies a day. Reading continues tomorrow."),
        ));
    }

    // The company is on record from the first read, with the site it was
    // read at, and the read counts toward the cap whatever comes of it.
    let mut tx = db.begin().await?;
    companies::note_site(&mut tx, account, company, site, source, now).await?;
    let company_id = companies::ensure(&mut tx, account, company, now).await?;
    sqlx::query(
        "INSERT INTO pipeline.company_reads (id, account_id, company_id, started_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(company_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let reading =
        neighbours
            .read_site(site, posting, now)
            .await
            .map_err(|failure| match failure {
                SiteFailure::Unreachable => Error::Unavailable("site_unreachable"),
                SiteFailure::RobotsDisallows => Error::Conflict(
                    "robots_disallows",
                    "The company's website asks robots not to read it, so nothing was read."
                        .to_owned(),
                ),
                SiteFailure::NothingReadable => Error::Conflict(
                    "no_pages_readable",
                    "None of the company's pages had text to read.".to_owned(),
                ),
            })?;

    // Two attempts at the same answer size: when the first is cut off, the
    // second is shown half the page text (the posting first, then the pages
    // by priority), never the same input with more room.
    let mut made: Option<(RawFacts, Vec<(String, String)>)> = None;
    let mut last = Error::Unavailable(applicamp_ai::AI_FAILED);
    for attempt in 0..2 {
        let (message, shown) =
            prompt::facts_message_within(company, &reading.pages, prompt::pages_budget(attempt));
        match ask_attempt::<RawFacts>(
            gateway,
            db,
            account,
            now,
            "pipeline_company_facts",
            &prompt::facts_schema(),
            prompt::FACTS_INSTRUCTIONS,
            &message,
            FACTS_TOKENS,
        )
        .await?
        {
            Attempt::Done(raw) => {
                // Ids and counts only.
                tracing::info!(attempt = attempt + 1, "a company read worked");
                made = Some((raw, shown));
                break;
            }
            Attempt::Retry(error) => last = error,
        }
    }
    let Some((raw, shown)) = made else {
        return Err(last);
    };
    let (facts, dropped) = judge::judge_facts(raw, &shown);

    store_read(
        db,
        company_id,
        &reading.pages,
        reading.blocked,
        &facts,
        dropped,
        now,
        opp,
    )
    .await?;
    // Ids and counts only.
    tracing::info!(
        prompt = prompt::FACTS_VERSION,
        pages = reading.pages.len(),
        blocked = reading.blocked,
        facts = facts.len(),
        dropped,
        "a company read"
    );
    Ok(())
}

/// Keep what a read found: the facts (a removed one stays removed), the text
/// of the pages they cite and the list of pages read; the posting's text as
/// the opportunity's description when it had none.
#[allow(clippy::too_many_arguments)]
async fn store_read(
    db: &PgPool,
    company: Uuid,
    pages: &[SitePage],
    blocked: u32,
    facts: &[NewFact],
    dropped: u32,
    now: DateTime<Utc>,
    opp: Option<&Opp>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    let removed: Vec<String> = sqlx::query_scalar(
        "SELECT text_key FROM pipeline.company_facts WHERE company_id = $1 AND removed",
    )
    .bind(company)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM pipeline.company_facts WHERE company_id = $1 AND NOT removed")
        .bind(company)
        .execute(&mut *tx)
        .await?;
    let account: Uuid =
        sqlx::query_scalar("SELECT account_id FROM pipeline.companies WHERE id = $1")
            .bind(company)
            .fetch_one(&mut *tx)
            .await?;
    for fact in facts {
        let key = squash(&fact.text);
        if removed.contains(&key) {
            continue;
        }
        if let Some(page) = pages.iter().find(|p| p.url == fact.url) {
            sqlx::query(
                "INSERT INTO pipeline.company_pages (company_id, url, fetched_at, kind, text)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (company_id, url) DO UPDATE
                    SET fetched_at = excluded.fetched_at, kind = excluded.kind, text = excluded.text",
            )
            .bind(company)
            .bind(&page.url)
            .bind(page.fetched_at)
            .bind(&page.kind)
            .bind(&page.text)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO pipeline.company_facts
                (id, company_id, account_id, text, quote, url, text_key, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (company_id, text_key) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(company)
        .bind(account)
        .bind(&fact.text)
        .bind(&fact.quote)
        .bind(&fact.url)
        .bind(key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    // Only the pages a fact cites keep their text.
    sqlx::query(
        "DELETE FROM pipeline.company_pages
          WHERE company_id = $1
            AND url NOT IN (SELECT url FROM pipeline.company_facts WHERE company_id = $1)",
    )
    .bind(company)
    .execute(&mut *tx)
    .await?;
    let read: Vec<ReadPage> = pages
        .iter()
        .map(|p| ReadPage {
            url: p.url.clone(),
            fetched_at: p.fetched_at,
            kind: p.kind.clone(),
        })
        .collect();
    sqlx::query(
        "UPDATE pipeline.companies
            SET read_at = $2, pages = $3, pages_blocked = $4, facts_dropped = $5, updated_at = $2
          WHERE id = $1",
    )
    .bind(company)
    .bind(now)
    .bind(sqlx::types::Json(&read))
    .bind(i32::try_from(blocked).unwrap_or(i32::MAX))
    .bind(i32::try_from(dropped).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await?;
    // A posting added by link has no text; the page just read is it.
    if let Some(opp) = opp
        && opp
            .description
            .as_deref()
            .is_none_or(|d| d.trim().is_empty())
        && let Some(posting) = pages.iter().find(|p| p.kind == "posting")
    {
        sqlx::query(
            "UPDATE pipeline.opportunities SET description = $2
              WHERE id = $1 AND (description IS NULL OR btrim(description) = '')",
        )
        .bind(opp.id)
        .bind(
            posting
                .text
                .chars()
                .take(MAX_DESCRIPTION)
                .collect::<String>(),
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Compare the posting with the company's facts and the profile (**one model
/// call**), unless that was done for this very posting text. Whether it
/// called the model.
async fn analyze(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp_id: Uuid,
    locale: Option<&str>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let opp = load(db, account, opp_id).await?;
    let posting = opp.description.clone().unwrap_or_default();
    if posting.trim().is_empty() {
        return Ok(false);
    }
    let key = fingerprint(&format!("{}\n{posting}", opp.title));
    let cached: Option<String> =
        sqlx::query_scalar("SELECT posting_key FROM pipeline.job_briefs WHERE opportunity_id = $1")
            .bind(opp.id)
            .fetch_optional(db)
            .await?;
    if cached.as_deref() == Some(key.as_str()) {
        return Ok(false);
    }
    let gateway = gateway(deps)?;
    let profile = digest(deps, db, account).await?.unwrap_or_default();
    let company = companies::find(db, account, &opp.company).await?;
    let facts: Vec<FactLine> = match &company {
        Some(company) => sqlx::query_as::<_, (String, String)>(
            "SELECT text, url FROM pipeline.company_facts
              WHERE company_id = $1 AND NOT removed ORDER BY created_at, id",
        )
        .bind(company.id)
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|(text, url)| FactLine { text, url })
        .collect(),
        None => Vec::new(),
    };
    let language = prompt::language(locale);
    let user =
        prompt::analysis_message(&opp.title, &posting, &facts, &profile, language.as_deref());
    let raw: RawAnalysis = ask(
        gateway,
        db,
        account,
        now,
        "pipeline_job_analysis",
        &prompt::analysis_schema(),
        prompt::ANALYSIS_INSTRUCTIONS,
        &user,
    )
    .await?;
    let searchable = format!("{}\n{posting}", opp.title);
    let analysis = judge::judge_analysis(raw, &searchable, &profile);
    store_analysis(db, account, opp.id, &key, &analysis, now).await?;
    tracing::info!(
        prompt = prompt::ANALYSIS_VERSION,
        wants = analysis.wants.len(),
        questions = analysis.questions.len(),
        dropped = analysis.dropped,
        "a job compared"
    );
    Ok(true)
}

async fn store_analysis(
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    key: &str,
    analysis: &Analysis,
    now: DateTime<Utc>,
) -> Result<()> {
    let wants: Vec<BriefWant> = analysis
        .wants
        .iter()
        .map(|w| BriefWant {
            label: w.label.clone(),
            quote: w.quote.clone(),
            level: w.level.as_str().to_owned(),
        })
        .collect();
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO pipeline.job_briefs (opportunity_id, account_id, posting_key, analyzed_at, wants)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (opportunity_id) DO UPDATE
            SET posting_key = excluded.posting_key, analyzed_at = excluded.analyzed_at,
                wants = excluded.wants",
    )
    .bind(opp)
    .bind(account)
    .bind(key)
    .bind(now)
    .bind(sqlx::types::Json(&wants))
    .execute(&mut *tx)
    .await?;
    // The same question keeps its state (skipped stays skipped); the others go.
    let keys: Vec<String> = analysis
        .questions
        .iter()
        .map(|q| squash(&q.question))
        .collect();
    sqlx::query(
        "DELETE FROM pipeline.brief_questions
          WHERE opportunity_id = $1 AND question_key <> ALL($2)",
    )
    .bind(opp)
    .bind(&keys)
    .execute(&mut *tx)
    .await?;
    for (seq, question) in analysis.questions.iter().enumerate() {
        sqlx::query(
            "INSERT INTO pipeline.brief_questions
                (id, opportunity_id, account_id, seq, question, question_key, about, skill,
                 created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (opportunity_id, question_key) DO UPDATE SET seq = excluded.seq",
        )
        .bind(Uuid::now_v7())
        .bind(opp)
        .bind(account)
        .bind(i32::try_from(seq).unwrap_or(i32::MAX))
        .bind(&question.question)
        .bind(squash(&question.question))
        .bind(question.about.and_then(|a| i32::try_from(a).ok()))
        .bind(&question.skill)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ---- endpoints ------------------------------------------------------------

/// The brief of an opportunity.
///
/// What a tailored CV for it would be based on, as it is now: the company's
/// site (or that the person has to type it), the facts read on it, what the
/// job wants most, the questions where the profile is thin, what each step
/// costs and what was spent, and what the app does by itself for it. **Never
/// calls the model and never fetches**; opening it may start the preparation
/// of a shortlisted job that was never prepared, or its next step once what it
/// waited on is done (it runs in the background). Another account's
/// opportunity is not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/brief",
    tag = "pipeline",
    operation_id = "pipeline.brief",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."), LanguageQuery),
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    Query(query): Query<LanguageQuery>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    let now = state.clock.now();
    let locale = query.locale.as_deref();
    prepare::nudge_in(&deps, &state.db, account, opp.id, locale, now).await;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// Type the company's website.
///
/// Only its host (and port) is kept, on the company: it is asked once, and
/// wins over every guess. The address must be one the server may ask (a public
/// https address). A different site than before forgets the facts read on the
/// old one.
#[utoipa::path(
    put,
    path = "/api/v1/opportunities/{id}/brief/site",
    tag = "pipeline",
    operation_id = "pipeline.brief_set_site",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    request_body = SiteRequest,
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 400, description = "`invalid_site`; `site_not_allowed`; `company_unknown`",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn set_site(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<SiteRequest>, JsonRejection>,
) -> Result<Json<Brief>> {
    let Json(request) =
        body.map_err(|rejection| Error::BadRequest("invalid_site", rejection.body_text()))?;
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    if opp.company.trim().is_empty() {
        return Err(unknown_company());
    }
    let invalid = || {
        Error::BadRequest(
            "invalid_site",
            "Write the company's website, starting with https://.".to_owned(),
        )
    };
    if request.url.chars().count() > MAX_URL {
        return Err(invalid());
    }
    let site = companies::origin_of(&request.url).ok_or_else(invalid)?;
    if !neighbours(&deps)?.site_allowed(&site) {
        return Err(Error::BadRequest(
            "site_not_allowed",
            "That address cannot be read. Use the company's public website.".to_owned(),
        ));
    }
    let now = state.clock.now();
    let before = companies::find(&state.db, account, &opp.company).await?;
    let mut tx = state.db.begin().await?;
    companies::note_site(
        &mut tx,
        account,
        &opp.company,
        &site,
        SiteSource::Person,
        now,
    )
    .await?;
    if before
        .as_ref()
        .and_then(|c| c.site.as_deref())
        .is_some_and(|old| old != site.as_str())
        && let Some(before) = &before
    {
        // Facts read on another site say nothing about this one.
        sqlx::query("DELETE FROM pipeline.company_facts WHERE company_id = $1")
            .bind(before.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM pipeline.company_pages WHERE company_id = $1")
            .bind(before.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE pipeline.companies SET read_at = NULL, pages = '[]'::jsonb,
                    pages_blocked = 0, facts_dropped = 0 WHERE id = $1",
        )
        .bind(before.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    prepare::nudge(&deps, &state.db, account, opp.id, now).await;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

fn unknown_company() -> Error {
    Error::BadRequest(
        "company_unknown",
        "This opportunity has no company name yet. Type it in first.".to_owned(),
    )
}

/// Read about the company and compare the job.
///
/// Reads the company's own pages (the posting, its home page and up to five
/// pages it links to about the company; the same site only, where robots.txt
/// allows) and makes the facts: **one model call**, reused for 30 days per
/// company unless `again=true`. Then compares the posting with those facts and
/// a compact profile: **one more call**, reused until the posting changes.
/// Each fact must quote the page it cites, or it is left out. At most
/// 20 company reads per account per UTC day, fetches included.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/brief/read",
    tag = "pipeline",
    operation_id = "pipeline.brief_read",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."), ReadQuery),
    responses(
        (status = 200, description = "The brief, after the read.", body = Brief),
        (status = 400, description = "`site_needed`; `company_unknown`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`robots_disallows`; `no_pages_readable`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`company_reads_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`site_unreachable`; `ai_unavailable`; \
            `ai_daily_limit_reached`; `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn read(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReadQuery>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    if opp.company.trim().is_empty() {
        return Err(unknown_company());
    }
    let now = state.clock.now();
    read_company(
        &deps,
        &state.db,
        account,
        &opp,
        query.again.unwrap_or(false),
        now,
    )
    .await?;
    analyze(&deps, &state.db, account, id, query.locale.as_deref(), now).await?;
    let opp = load(&state.db, account, id).await?;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// The company a fact of opportunity `opp` belongs to, if it does.
async fn fact_company(db: &PgPool, account: Uuid, opp: &Opp, fact: Uuid) -> Result<Uuid> {
    let company = companies::find(db, account, &opp.company)
        .await?
        .ok_or(Error::NotFound)?;
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM pipeline.company_facts WHERE id = $1 AND company_id = $2",
    )
    .bind(fact)
    .bind(company.id)
    .fetch_optional(db)
    .await?;
    found.ok_or(Error::NotFound)
}

async fn set_removed(
    state: &AppState,
    deps: &PipelineDeps,
    account: Uuid,
    id: Uuid,
    fact: Uuid,
    removed: bool,
) -> Result<Json<Brief>> {
    let opp = load(&state.db, account, id).await?;
    fact_company(&state.db, account, &opp, fact).await?;
    sqlx::query("UPDATE pipeline.company_facts SET removed = $2 WHERE id = $1")
        .bind(fact)
        .bind(removed)
        .execute(&state.db)
        .await?;
    Ok(Json(
        build(deps, &state.db, account, &opp, state.clock.now()).await?,
    ))
}

/// Remove a fact: "Wrong, remove".
///
/// It stays removed on later reads, and Undo (`…/restore`) brings it back.
#[utoipa::path(
    delete,
    path = "/api/v1/opportunities/{id}/brief/facts/{fact_id}",
    tag = "pipeline",
    operation_id = "pipeline.brief_remove_fact",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("fact_id" = Uuid, Path, description = "The fact.")),
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn remove_fact(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, fact)): Path<(Uuid, Uuid)>,
) -> Result<Json<Brief>> {
    set_removed(&state, &deps, principal.account_id, id, fact, true).await
}

/// Undo a removal: the fact is back.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/brief/facts/{fact_id}/restore",
    tag = "pipeline",
    operation_id = "pipeline.brief_restore_fact",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("fact_id" = Uuid, Path, description = "The fact.")),
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn restore_fact(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, fact)): Path<(Uuid, Uuid)>,
) -> Result<Json<Brief>> {
    set_removed(&state, &deps, principal.account_id, id, fact, false).await
}

async fn open_question(
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    question: Uuid,
) -> Result<QuestionRow> {
    let row: QuestionRow = sqlx::query_as(
        "SELECT id, question, about, skill, state FROM pipeline.brief_questions
          WHERE id = $1 AND opportunity_id = $2 AND account_id = $3",
    )
    .bind(question)
    .bind(opp)
    .bind(account)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    if row.state != QuestionState::Open.as_str() {
        return Err(Error::Conflict(
            "question_closed",
            "This question was already answered or put away.".to_owned(),
        ));
    }
    Ok(row)
}

async fn set_state(db: &PgPool, question: Uuid, state: QuestionState) -> Result<()> {
    sqlx::query("UPDATE pipeline.brief_questions SET state = $2 WHERE id = $1")
        .bind(question)
        .bind(state.as_str())
        .execute(db)
        .await?;
    Ok(())
}

/// Answer a question.
///
/// **One small model call** reads the answer as Grill me does, with the same
/// checks: each fact quotes the person's own words and says nothing they or
/// their profile do not. The facts wait on the position the question is about
/// (the most recent one when it names none: `defaulted`), to be kept, fixed or
/// dropped with Grill me's endpoints (`/api/v1/profile/grill/proposals/{id}/…`).
/// Nothing reaches the profile until a click.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/brief/questions/{question_id}/answer",
    tag = "pipeline",
    operation_id = "pipeline.brief_answer",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("question_id" = Uuid, Path, description = "The question."), LanguageQuery),
    request_body = AnswerRequest,
    responses(
        (status = 200, description = "What the answer came to.", body = AnswerResult),
        (status = 400, description = "`invalid_answer`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`question_closed`; `profile_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn answer(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, question)): Path<(Uuid, Uuid)>,
    Query(query): Query<LanguageQuery>,
    body: std::result::Result<Json<AnswerRequest>, JsonRejection>,
) -> Result<Json<AnswerResult>> {
    let Json(request) =
        body.map_err(|rejection| Error::BadRequest("invalid_answer", rejection.body_text()))?;
    let text = request.text.trim().to_owned();
    if text.is_empty() || text.chars().count() > MAX_ANSWER {
        return Err(Error::BadRequest(
            "invalid_answer",
            format!("Write between 1 and {MAX_ANSWER} characters."),
        ));
    }
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    let row = open_question(&state.db, account, opp.id, question).await?;
    let profile = digest(&deps, &state.db, account).await?;
    let needed = || {
        Error::Conflict(
            "profile_needed",
            "Add a position to your profile first: the answer needs one to go on.".to_owned(),
        )
    };
    let profile = profile.ok_or_else(needed)?;
    let named = row
        .about
        .and_then(|a| usize::try_from(a).ok())
        .filter(|a| profile.roles.iter().any(|r| r.index == *a));
    let position = named.or(profile.latest).ok_or_else(needed)?;
    let now = state.clock.now();
    let answered = neighbours(&deps)?
        .answer_about(
            &state.db,
            account,
            now,
            position,
            &row.question,
            &text,
            prompt::language(query.locale.as_deref()).as_deref(),
        )
        .await?;
    let proposals: Vec<Uuid> = answered.facts.iter().map(|f| f.id).collect();
    sqlx::query("UPDATE pipeline.brief_questions SET state = $2, proposals = $3 WHERE id = $1")
        .bind(row.id)
        .bind(QuestionState::Answered.as_str())
        .bind(&proposals)
        .execute(&state.db)
        .await?;
    prepare::nudge(&deps, &state.db, account, opp.id, now).await;
    let mut view = row.view(Some(&profile));
    view.state = QuestionState::Answered;
    Ok(Json(AnswerResult {
        question: view,
        role: RoleRef {
            position: u32::try_from(answered.position).map_err(Error::internal)?,
            title: answered.title,
            company: answered.company,
        },
        defaulted: named.is_none(),
        proposals: answered
            .facts
            .into_iter()
            .map(|f| AnswerProposal {
                id: f.id,
                text: f.text,
                quote: f.quote,
                area: f.area,
                expires_at: f.expires_at,
            })
            .collect(),
        dropped: answered.dropped,
    }))
}

/// "I don't have it."
///
/// For a question about a skill: the skill is recorded as not theirs (like a
/// dismissed suggestion), and neither it nor this question is asked about
/// again. No model call. A question with no skill is a 400.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/brief/questions/{question_id}/dont-have",
    tag = "pipeline",
    operation_id = "pipeline.brief_dont_have",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("question_id" = Uuid, Path, description = "The question.")),
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 400, description = "`no_skill`: the question is not about a skill",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`question_closed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn dont_have(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, question)): Path<(Uuid, Uuid)>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    let row = open_question(&state.db, account, opp.id, question).await?;
    let Some(skill) = row.skill.as_deref() else {
        return Err(Error::BadRequest(
            "no_skill",
            "This question is not about a skill, so there is nothing to put away.".to_owned(),
        ));
    };
    let now = state.clock.now();
    neighbours(&deps)?
        .dismiss_skill(&state.db, account, skill, now)
        .await?;
    set_state(&state.db, row.id, QuestionState::DontHave).await?;
    prepare::nudge(&deps, &state.db, account, opp.id, now).await;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// Skip a question: it is hidden for this opportunity. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/brief/questions/{question_id}/skip",
    tag = "pipeline",
    operation_id = "pipeline.brief_skip",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("question_id" = Uuid, Path, description = "The question.")),
    responses(
        (status = 200, description = "The brief.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`question_closed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn skip(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, question)): Path<(Uuid, Uuid)>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    let row = open_question(&state.db, account, opp.id, question).await?;
    set_state(&state.db, row.id, QuestionState::Skipped).await?;
    let now = state.clock.now();
    prepare::nudge(&deps, &state.db, account, opp.id, now).await;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}
