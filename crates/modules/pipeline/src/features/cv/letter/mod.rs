//! The cover letter, the CV's second document (#45).
//!
//! For an opportunity whose CV is approved
//! (`/api/v1/opportunities/{id}/letter…`), built on the CV's pipeline:
//!
//! - **Draft** (one model call, on a click): the model is shown the posting,
//!   what the job wants, the company facts the person has not removed, the
//!   profile cut into sources with ids, the words of the approved CV (so the
//!   two agree) and the job's own questions, and answers paragraphs of
//!   sentences and the answers, each citing its sources ([`prompt`]). It never
//!   sees a name or how to reach the person: the server prints those itself.
//! - **Fact check** (no model, [`check`]): a sentence says only what its
//!   sources say; a company fact may be cited anywhere to say why the company
//!   appeals but never backs a claim about the person; praise nobody wrote is
//!   flagged; an answer the profile cannot support comes back empty for the
//!   person to write. Whatever fails is **flagged, never dropped**, and
//!   **Approve stays off** while anything is open.
//! - **Review**: edit a sentence (checked again), say "it's true" (the
//!   person's words become a fact kept on the profile and a source), remove it,
//!   write their own answer to a job question.
//! - **Approve** freezes the draft and renders it with the CV's Typst stack
//!   ([`pdf`]): one page, read back, kept as a version. The answers are text
//!   to copy, not part of the PDF.
//!
//! Every model call starts with a click. Letter drafts are counted apart from
//! CV drafts: at most [`CAP_PER_OPPORTUNITY`] per opportunity and
//! [`CAP_PER_DAY`] per account per UTC day, all through the AI budget.
//!
//! It lives under the CV because it shares its sources, check primitives,
//! PDF world and read-back, and the same neighbours; the two documents have
//! their own tables because a letter's content is a different shape.

pub mod check;
pub mod judge;
pub mod model;
pub mod pdf;
pub mod prompt;
pub mod questions;
pub mod view;

use applicamp_ai::Role;
use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use self::{
    check::Checks,
    judge::{MAX_ANSWER, MAX_SENTENCE, RawLetter},
    model::{Content, LetterSize},
    view::{DraftParts, LetterDraftView, LetterVersion},
};
use super::{
    AREAS, ConfirmRequest, ExpireCvDrafts, LIFETIME, MAX_FACT, Opp, counted, gateway_ready,
    json_body, language_of, load,
    model::{CompanyFact, Frame, Source, SourceKind, short},
    neighbours,
    pdf::{Refusal, data::lang_of},
    versions::{PdfFile, PdfQuery, disposition, file_stem},
    view::DraftStatus,
    wants_of,
};
use crate::{PipelineDeps, companies, features::brief};

/// Letter drafts one opportunity can have, ever (until they are swept). Not
/// the CV's: a CV draft does not count here.
pub const CAP_PER_OPPORTUNITY: i64 = 10;
/// Letter drafts one account can start in a UTC day.
pub const CAP_PER_DAY: i64 = 20;
/// About what a draft costs, in euros: one call.
pub const DRAFT_EUR: f64 = 0.002;

// ---- views ----------------------------------------------------------------

/// Why a letter cannot be drafted now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LetterBlocked {
    /// The opportunity is still Found: shortlist it first.
    StageTooEarly,
    /// The CV for this job is not approved yet: the letter is written next to it.
    NoApprovedCv,
    /// This opportunity has its letter drafts.
    PerOpportunityCap,
    /// The account's letter drafts for today.
    DailyCap,
    /// No AI model is set up.
    AiUnavailable,
}

/// Whether "Write the cover letter" is on.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct CanWrite {
    pub allowed: bool,
    pub reason: Option<LetterBlocked>,
}

/// The caps and the spend.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct LetterCaps {
    /// Letter drafts this opportunity has had.
    pub drafts_here: u32,
    pub drafts_here_cap: u32,
    /// Letter drafts the account started today (UTC).
    pub drafts_today: u32,
    pub drafts_today_cap: u32,
    pub today_eur: f64,
    pub day_limit_eur: Option<f64>,
    pub month_eur: f64,
    pub month_limit_eur: Option<f64>,
}

/// The cover letter of an opportunity.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Letter {
    pub opportunity_id: Uuid,
    /// The latest draft that was not discarded (approved or not); none before
    /// the first.
    pub draft: Option<LetterDraftView>,
    /// The PDF versions, newest first.
    pub versions: Vec<LetterVersion>,
    pub can_write: CanWrite,
    pub caps: LetterCaps,
    /// About what one draft costs, in euros.
    pub draft_eur: f64,
    /// The version of the approved CV a new letter would sit next to.
    pub cv_version: Option<u32>,
    /// The job's own questions, read from its posting.
    pub questions: Vec<String>,
}

/// What to make a draft of.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LetterDraftRequest {
    /// A language tag such as `pt-PT`; the posting's when it says, else
    /// English.
    #[serde(default)]
    pub language: Option<String>,
    /// `short` (about 180 words, the default) or `standard` (about 300).
    #[serde(default)]
    pub size: Option<LetterSize>,
}

/// A change to one sentence: its new text, or its removal.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SentenceChange {
    /// The new text. It is checked against the same sources.
    #[serde(default)]
    pub text: Option<String>,
    /// Remove the sentence.
    #[serde(default)]
    pub remove: Option<bool>,
}

/// The person's own answer to a job question, or its removal.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerChange {
    /// The answer, in the person's words. It is theirs, so it is not checked.
    #[serde(default)]
    pub text: Option<String>,
    /// Leave the question out.
    #[serde(default)]
    pub remove: Option<bool>,
}

// ---- storage --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    n: i32,
    language: String,
    size: String,
    cv_version: Option<i32>,
    status: String,
    version: Option<i32>,
    content: sqlx::types::Json<Content>,
    frame: sqlx::types::Json<Frame>,
    checks: sqlx::types::Json<Checks>,
    created_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
}

/// A select of a letter draft's columns, with `$tail` after the table.
macro_rules! select_draft {
    ($tail:literal) => {
        concat!(
            "SELECT id, n, language, size, cv_version, status, version, content, frame, checks, \
             created_at, approved_at, expires_at FROM pipeline.letter_drafts ",
            $tail
        )
    };
}

async fn latest(db: &PgPool, account: Uuid, opp: Uuid, now: DateTime<Utc>) -> Result<Option<Row>> {
    Ok(sqlx::query_as(select_draft!(
        "WHERE opportunity_id = $1 AND account_id = $2 AND status <> 'discarded'
            AND (expires_at IS NULL OR expires_at > $3)
          ORDER BY n DESC LIMIT 1"
    ))
    .bind(opp)
    .bind(account)
    .bind(now)
    .fetch_optional(db)
    .await?)
}

async fn by_n(db: &PgPool, account: Uuid, opp: Uuid, n: u32, now: DateTime<Utc>) -> Result<Row> {
    sqlx::query_as(select_draft!(
        "WHERE opportunity_id = $1 AND account_id = $2 AND n = $3
            AND (expires_at IS NULL OR expires_at > $4)"
    ))
    .bind(opp)
    .bind(account)
    .bind(i32::try_from(n).map_err(|_| Error::NotFound)?)
    .bind(now)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)
}

async fn drafts_here(db: &PgPool, opp: Uuid) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM pipeline.letter_drafts WHERE opportunity_id = $1")
            .bind(opp)
            .fetch_one(db)
            .await?,
    )
}

async fn drafts_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.letter_drafts WHERE account_id = $1 AND created_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

/// The approved CV a letter sits next to: its version and its words.
struct ApprovedCv {
    version: i32,
    content: sqlx::types::Json<super::model::Content>,
}

async fn approved_cv(db: &PgPool, account: Uuid, opp: Uuid) -> Result<Option<ApprovedCv>> {
    let row: Option<(i32, sqlx::types::Json<super::model::Content>)> = sqlx::query_as(
        "SELECT version, content FROM pipeline.cv_drafts
          WHERE opportunity_id = $1 AND account_id = $2 AND status = 'approved'
            AND version IS NOT NULL
          ORDER BY n DESC LIMIT 1",
    )
    .bind(opp)
    .bind(account)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(version, content)| ApprovedCv { version, content }))
}

/// Whether a letter can be drafted now, and why not.
fn can_write(stage: &str, cv: bool, here: i64, today: i64, ai: bool) -> CanWrite {
    let reason = if stage == "found" {
        Some(LetterBlocked::StageTooEarly)
    } else if !cv {
        Some(LetterBlocked::NoApprovedCv)
    } else if !ai {
        Some(LetterBlocked::AiUnavailable)
    } else if here >= CAP_PER_OPPORTUNITY {
        Some(LetterBlocked::PerOpportunityCap)
    } else if today >= CAP_PER_DAY {
        Some(LetterBlocked::DailyCap)
    } else {
        None
    };
    CanWrite {
        allowed: reason.is_none(),
        reason,
    }
}

fn draft_view(row: &Row, opp: &Opp) -> LetterDraftView {
    view::draft_view(
        &DraftParts {
            n: counted(i64::from(row.n)),
            status: DraftStatus::parse(&row.status),
            version: row.version.map(|v| counted(i64::from(v))),
            language: &row.language,
            size: LetterSize::parse(&row.size).unwrap_or_default(),
            cv_version: row.cv_version.map(|v| counted(i64::from(v))),
            created_at: row.created_at,
            approved_at: row.approved_at,
            expires_at: row.expires_at,
            content: &row.content.0,
            frame: &row.frame.0,
            checks: &row.checks.0,
        },
        &opp.company,
    )
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    n: i32,
    version: i32,
    language: String,
    approved_at: Option<DateTime<Utc>>,
    pages: i16,
    size_bytes: i32,
    created_at: DateTime<Utc>,
    downloaded_at: Option<DateTime<Utc>>,
    checks: sqlx::types::Json<super::pdf::readback::PdfChecks>,
}

/// The PDF versions of `opp`, newest first (the PDF itself is not read).
async fn versions_of(db: &PgPool, account: Uuid, opp: Uuid) -> Result<Vec<LetterVersion>> {
    let rows: Vec<VersionRow> = sqlx::query_as(
        "SELECT d.n, v.version, v.language, d.approved_at, v.pages, v.size_bytes, v.created_at,
                v.downloaded_at, v.checks
           FROM pipeline.letter_versions v JOIN pipeline.letter_drafts d ON d.id = v.draft_id
          WHERE v.opportunity_id = $1 AND v.account_id = $2
          ORDER BY v.version DESC",
    )
    .bind(opp)
    .bind(account)
    .fetch_all(db)
    .await?;
    let newest = rows.first().map(|r| r.version);
    Ok(rows
        .into_iter()
        .map(|r| LetterVersion {
            n: counted(i64::from(r.n)),
            version: counted(i64::from(r.version)),
            language: r.language,
            approved_at: r.approved_at.unwrap_or(r.created_at),
            pages: counted(i64::from(r.pages)),
            size_bytes: u64::try_from(r.size_bytes).unwrap_or(0),
            created_at: r.created_at,
            downloaded_at: r.downloaded_at,
            current: Some(r.version) == newest,
            checks: r.checks.0,
        })
        .collect())
}

/// The cover letter of `opp`, as it is now. Never calls the model.
async fn build(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: &Opp,
    now: DateTime<Utc>,
) -> Result<Letter> {
    let draft = latest(db, account, opp.id, now)
        .await?
        .map(|row| draft_view(&row, opp));
    let versions = versions_of(db, account, opp.id).await?;
    let cv = approved_cv(db, account, opp.id).await?;
    let here = drafts_here(db, opp.id).await?;
    let today = drafts_today(db, account, now).await?;
    let spend = match &deps.ai {
        Some(ai) => ai.spend(db, account, now).await?,
        None => applicamp_ai::Spend::default(),
    };
    Ok(Letter {
        opportunity_id: opp.id,
        draft,
        versions,
        can_write: can_write(&opp.stage, cv.is_some(), here, today, gateway_ready(deps)),
        caps: LetterCaps {
            drafts_here: counted(here),
            drafts_here_cap: counted(CAP_PER_OPPORTUNITY),
            drafts_today: counted(today),
            drafts_today_cap: counted(CAP_PER_DAY),
            today_eur: spend.today_eur,
            day_limit_eur: spend.day_limit_eur,
            month_eur: spend.month_eur,
            month_limit_eur: spend.month_limit_eur,
        },
        draft_eur: DRAFT_EUR,
        cv_version: cv.map(|c| counted(i64::from(c.version))),
        questions: questions::parse(opp.description.as_deref().unwrap_or_default()),
    })
}

/// Write a draft's changes, and push its expiry 7 days from `now`.
async fn save(
    db: &PgPool,
    row: &Row,
    content: &Content,
    frame: &Frame,
    checks: &Checks,
    now: DateTime<Utc>,
) -> Result<()> {
    let expires_at = now + LIFETIME;
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE pipeline.letter_drafts
            SET content = $2, frame = $3, checks = $4, updated_at = $5, expires_at = $6
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(row.id)
    .bind(sqlx::types::Json(content))
    .bind(sqlx::types::Json(frame))
    .bind(sqlx::types::Json(checks))
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireCvDrafts {}, expires_at).await?;
    tx.commit().await?;
    Ok(())
}

/// A draft the person can still change.
async fn open_draft(
    state: &AppState,
    account: Uuid,
    id: Uuid,
    n: u32,
    now: DateTime<Utc>,
) -> Result<(Opp, Row)> {
    let opp = load(&state.db, account, id).await?;
    let row = by_n(&state.db, account, opp.id, n, now).await?;
    if DraftStatus::parse(&row.status) != DraftStatus::Draft {
        return Err(Error::Conflict(
            "draft_frozen",
            "This letter was approved or put away, so it cannot change. Write it again.".to_owned(),
        ));
    }
    Ok((opp, row))
}

fn invalid_text(most: usize) -> Error {
    Error::BadRequest(
        "invalid_text",
        format!("Write between 1 and {most} characters."),
    )
}

/// The checks of `content` after a change, saved with it.
#[allow(clippy::too_many_arguments)]
async fn recheck_and_save(
    state: &AppState,
    deps: &PipelineDeps,
    account: Uuid,
    opp: &Opp,
    row: &Row,
    content: &Content,
    frame: &Frame,
    now: DateTime<Utc>,
) -> Result<Json<Letter>> {
    let checks = check::check(content, frame);
    save(&state.db, row, content, frame, &checks, now).await?;
    Ok(Json(build(deps, &state.db, account, opp, now).await?))
}

// ---- endpoints ------------------------------------------------------------

/// The cover letter of an opportunity.
///
/// The latest draft with each paragraph's tags and sources and each
/// sentence's reasons, the job's own questions and their answers, the PDF
/// versions, the caps and the spend. **Never calls the model.** Another
/// account's opportunity is not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/letter",
    tag = "pipeline",
    operation_id = "pipeline.letter",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    responses(
        (status = 200, description = "The letter.", body = Letter),
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
) -> Result<Json<Letter>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    Ok(Json(
        build(&deps, &state.db, account, &opp, state.clock.now()).await?,
    ))
}

/// Write the cover letter for this job.
///
/// **One model call.** From the posting, what the job wants, the company
/// facts not removed, the profile, the approved CV's words and the job's own
/// questions; each sentence cites the sources it comes from, and the server
/// then checks that it says nothing they do not. The model never sees a name
/// or contact details. Needs the CV of this job approved. At most 10 letter
/// drafts per opportunity and 20 per account per UTC day, counted apart from
/// CV drafts; nothing retries past a limit. A new draft puts the earlier
/// unapproved one away.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/letter/drafts",
    tag = "pipeline",
    operation_id = "pipeline.letter_draft",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    request_body = LetterDraftRequest,
    responses(
        (status = 200, description = "The letter, with the new draft.", body = Letter),
        (status = 400, description = "`invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`stage_too_early`; `no_approved_cv`; `profile_needed`",
            body = Problem, content_type = "application/problem+json"),
        (status = 429, description = "`letter_drafts_per_opportunity_cap`; \
            `letter_drafts_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    body: Option<Json<LetterDraftRequest>>,
) -> Result<Json<Letter>> {
    let request = body.map(|Json(b)| b).unwrap_or_default();
    let account = principal.account_id;
    let now = state.clock.now();
    let db = &state.db;
    let opp = load(db, account, id).await?;
    if opp.stage == "found" {
        return Err(Error::Conflict(
            "stage_too_early",
            "Shortlist this job first: a letter is written for the jobs you are going after."
                .to_owned(),
        ));
    }
    let cv = approved_cv(db, account, opp.id).await?.ok_or_else(|| {
        Error::Conflict(
            "no_approved_cv",
            "Approve your CV for this job first: the letter is written next to it.".to_owned(),
        )
    })?;
    let gateway = match &deps.ai {
        Some(gateway) if gateway.is_configured(Role::Complete) => gateway,
        _ => return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    };
    let neighbours = neighbours(&deps)?;
    if drafts_here(db, opp.id).await? >= CAP_PER_OPPORTUNITY {
        return Err(Error::TooManyRequests(
            "letter_drafts_per_opportunity_cap",
            format!(
                "You can write {CAP_PER_OPPORTUNITY} letter drafts for one job. Approve or edit one."
            ),
        ));
    }
    if drafts_today(db, account, now).await? >= CAP_PER_DAY {
        return Err(Error::TooManyRequests(
            "letter_drafts_daily_cap",
            format!("You can write {CAP_PER_DAY} letter drafts a day. Writing continues tomorrow."),
        ));
    }
    let profile = neighbours
        .cv_profile(db, account)
        .await?
        .filter(|p| !p.positions.is_empty())
        .ok_or_else(|| {
            Error::Conflict(
                "profile_needed",
                "Add a position to your profile first: the letter is made from it.".to_owned(),
            )
        })?;
    let wants = wants_of(db, opp.id).await?;
    let facts: Vec<CompanyFact> = match companies::find(db, account, &opp.company).await? {
        Some(company) => sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, text, url FROM pipeline.company_facts
              WHERE company_id = $1 AND NOT removed ORDER BY created_at, id",
        )
        .bind(company.id)
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|(id, text, url)| CompanyFact {
            id: id.to_string(),
            text,
            url,
        })
        .collect(),
        None => Vec::new(),
    };
    let frame = Frame::build(&profile, &opp.company, &facts);
    let language = language_of(request.language.as_deref(), &opp);
    let size = request.size.unwrap_or_default();
    let posting = opp.description.clone().unwrap_or_default();
    let questions = questions::parse(&posting);
    let cv_words = cv_words(&cv.content.0);
    let user = prompt::message(&prompt::Input {
        title: &opp.title,
        company: &opp.company,
        posting: &posting,
        wants: &wants,
        frame: &frame,
        cv: &cv_words,
        questions: &questions,
        language: &language,
        size,
    });
    let raw: RawLetter = brief::ask(
        gateway,
        db,
        account,
        now,
        "pipeline_cover_letter",
        &prompt::schema(),
        prompt::INSTRUCTIONS,
        &user,
    )
    .await?;
    let content = judge::judge(raw, &questions, &profile)
        .ok_or(Error::Unavailable(applicamp_ai::AI_FAILED))?;
    let checks = check::check(&content, &frame);

    let expires_at = now + LIFETIME;
    let mut tx = db.begin().await?;
    let n: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(n), 0) + 1 FROM pipeline.letter_drafts WHERE opportunity_id = $1",
    )
    .bind(opp.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE pipeline.letter_drafts SET status = 'discarded', updated_at = $2
          WHERE opportunity_id = $1 AND status = 'draft'",
    )
    .bind(opp.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO pipeline.letter_drafts
            (id, account_id, opportunity_id, n, language, size, cv_version, status, content,
             frame, checks, created_at, updated_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'draft', $8, $9, $10, $11, $11, $12)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(opp.id)
    .bind(n)
    .bind(&language)
    .bind(size.as_str())
    .bind(cv.version)
    .bind(sqlx::types::Json(&content))
    .bind(sqlx::types::Json(&frame))
    .bind(sqlx::types::Json(&checks))
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireCvDrafts {}, expires_at).await?;
    tx.commit().await?;
    // Ids and counts only.
    tracing::info!(
        prompt = prompt::VERSION,
        paragraphs = content.paragraphs.len(),
        answers = content.answers.len(),
        flagged = checks.len(),
        "a cover letter drafted"
    );
    Ok(Json(build(&deps, db, account, &opp, now).await?))
}

/// The words of the approved CV the letter is written next to: its headline,
/// summary, bullets and skills. No name, no contact.
fn cv_words(cv: &super::model::Content) -> String {
    let bullets: Vec<String> = cv
        .experience
        .iter()
        .flat_map(|r| r.bullets.iter().map(|b| b.text.clone()))
        .chain((!cv.skills.is_empty()).then(|| {
            format!(
                "Skills: {}",
                cv.skills
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }))
        .collect();
    prompt::cv_lines(
        cv.headline.as_ref().map(|l| l.text.as_str()),
        cv.summary.as_ref().map(|l| l.text.as_str()),
        &bullets,
    )
}

/// Change or remove one sentence.
///
/// The new text is checked against the sentence's sources again; removing it
/// takes it out of the letter. Only a draft can change.
#[utoipa::path(
    patch,
    path = "/api/v1/opportunities/{id}/letter/drafts/{n}/sentences/{sentence_id}",
    tag = "pipeline",
    operation_id = "pipeline.letter_sentence",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."),
        ("sentence_id" = String, Path, description = "The sentence.")),
    request_body = SentenceChange,
    responses(
        (status = 200, description = "The letter, after the change.", body = Letter),
        (status = 400, description = "`invalid_change`; `invalid_text`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such opportunity, draft or sentence",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn change_sentence(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n, sentence)): Path<(Uuid, u32, String)>,
    body: std::result::Result<Json<SentenceChange>, JsonRejection>,
) -> Result<Json<Letter>> {
    let change = json_body(body, "invalid_change")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let mut content = row.content.0.clone();
    let (p, s) = content.locate(&sentence).ok_or(Error::NotFound)?;
    match (change.text, change.remove) {
        (Some(text), None | Some(false)) => {
            let text =
                judge::edited(&text, MAX_SENTENCE).ok_or_else(|| invalid_text(MAX_SENTENCE))?;
            if let Some(target) = content
                .paragraphs
                .get_mut(p)
                .and_then(|para| para.sentences.get_mut(s))
            {
                target.text = text;
                target.edited = true;
            }
        }
        (None, Some(true)) => {
            if let Some(paragraph) = content.paragraphs.get_mut(p) {
                paragraph.sentences.remove(s);
                if paragraph.sentences.is_empty() {
                    content.paragraphs.remove(p);
                }
            }
        }
        _ => {
            return Err(Error::BadRequest(
                "invalid_change",
                "Send either the new `text` or `remove: true`.".to_owned(),
            ));
        }
    }
    recheck_and_save(
        &state,
        &deps,
        account,
        &opp,
        &row,
        &content,
        &row.frame.0,
        now,
    )
    .await
}

/// "It's true": keep the person's own words as a fact on their profile and
/// cite it from the sentence.
///
/// The words (up to 300 characters) become a fact kept on the position the
/// sentence is about (else the most recent), a source the sentence cites, and
/// the sentence is checked again with it. The profile keeps them for every
/// later letter and CV.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/letter/drafts/{n}/sentences/{sentence_id}/confirm",
    tag = "pipeline",
    operation_id = "pipeline.letter_confirm",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."),
        ("sentence_id" = String, Path, description = "The sentence.")),
    request_body = ConfirmRequest,
    responses(
        (status = 200, description = "The letter, after the sentence was checked again.",
            body = Letter),
        (status = 400, description = "`invalid_fact`; `invalid_area`; `no_position`",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`; `profile_changed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn confirm_sentence(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n, sentence)): Path<(Uuid, u32, String)>,
    body: std::result::Result<Json<ConfirmRequest>, JsonRejection>,
) -> Result<Json<Letter>> {
    let request = json_body(body, "invalid_fact")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let neighbours = neighbours(&deps)?;
    let mut content = row.content.0.clone();
    let mut frame = row.frame.0.clone();
    let (p, s) = content.locate(&sentence).ok_or(Error::NotFound)?;
    let area = request.area.as_deref().unwrap_or("results");
    if !AREAS.contains(&area) {
        return Err(Error::BadRequest(
            "invalid_area",
            "That is not something a fact can be about.".to_owned(),
        ));
    }
    let text = request.text.as_deref().map(str::trim).unwrap_or_default();
    if text.is_empty() || text.chars().count() > MAX_FACT {
        return Err(Error::BadRequest(
            "invalid_fact",
            format!("Write between 1 and {MAX_FACT} characters."),
        ));
    }
    // The position the sentence is about: its first profile source's, else
    // the most recent one.
    let cited = content
        .paragraphs
        .get(p)
        .and_then(|para| para.sentences.get(s))
        .map(|sen| sen.sources.clone())
        .unwrap_or_default();
    let position = cited
        .iter()
        .filter_map(|id| frame.get(id))
        .find_map(|source| source.position)
        .or(frame.latest)
        .ok_or_else(|| {
            Error::BadRequest(
                "no_position",
                "The profile has no position to put it on.".to_owned(),
            )
        })?;
    let info = frame.position(position).cloned().ok_or(Error::NotFound)?;
    let fact = neighbours
        .keep_fact(
            &state,
            account,
            position,
            info.title.as_deref(),
            info.company.as_deref(),
            text,
            area,
        )
        .await?;
    let source = format!("w{position}.f{}", short(&fact));
    let order = frame
        .of_position(position)
        .filter_map(|s| s.order)
        .max()
        .map_or(0, |o| o + 1);
    frame.sources.push(Source {
        id: source.clone(),
        kind: SourceKind::Fact,
        text: text.to_owned(),
        position: Some(position),
        order: Some(order),
        url: None,
    });
    if let Some(target) = content
        .paragraphs
        .get_mut(p)
        .and_then(|para| para.sentences.get_mut(s))
        && !target.sources.contains(&source)
    {
        target.sources.push(source);
    }
    recheck_and_save(&state, &deps, account, &opp, &row, &content, &frame, now).await
}

/// Write your own answer to one of the job's questions, or leave it out.
///
/// The answer is the person's own words, so it is not checked against the
/// profile; it is text to copy into the application, not part of the PDF.
#[utoipa::path(
    patch,
    path = "/api/v1/opportunities/{id}/letter/drafts/{n}/answers/{answer_id}",
    tag = "pipeline",
    operation_id = "pipeline.letter_answer",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."),
        ("answer_id" = String, Path, description = "The answer.")),
    request_body = AnswerChange,
    responses(
        (status = 200, description = "The letter, after the change.", body = Letter),
        (status = 400, description = "`invalid_change`; `invalid_text`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn change_answer(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n, answer)): Path<(Uuid, u32, String)>,
    body: std::result::Result<Json<AnswerChange>, JsonRejection>,
) -> Result<Json<Letter>> {
    let change = json_body(body, "invalid_change")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let mut content = row.content.0.clone();
    let at = content
        .answers
        .iter()
        .position(|a| a.id == answer)
        .ok_or(Error::NotFound)?;
    match (change.text, change.remove) {
        (Some(text), None | Some(false)) => {
            let text = judge::edited(&text, MAX_ANSWER).ok_or_else(|| invalid_text(MAX_ANSWER))?;
            if let Some(target) = content.answers.get_mut(at) {
                target.text = text;
                target.sources.clear();
                target.own = true;
            }
        }
        (None, Some(true)) => {
            content.answers.remove(at);
        }
        _ => {
            return Err(Error::BadRequest(
                "invalid_change",
                "Send either the new `text` or `remove: true`.".to_owned(),
            ));
        }
    }
    recheck_and_save(
        &state,
        &deps,
        account,
        &opp,
        &row,
        &content,
        &row.frame.0,
        now,
    )
    .await
}

/// The next version number of `opp`, with the opportunity's letter versions
/// locked against a second request choosing the same.
async fn next_version(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, opp: Uuid) -> Result<i32> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("letter_versions:{opp}"))
        .execute(&mut **tx)
        .await?;
    Ok(sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM pipeline.letter_versions
          WHERE opportunity_id = $1",
    )
    .bind(opp)
    .fetch_one(&mut **tx)
    .await?)
}

/// Approve the letter and make its PDF.
///
/// Checked again now: refused while anything is flagged or a job question is
/// still open (`letter_flagged`). The letter is then rendered on one page with
/// the CV's template stack, read back (every paragraph, the name and the email
/// must come out of the PDF as text), and kept as the next version in the
/// same transaction that approves; if any step fails nothing is approved.
/// **No model call.** The answers to the job's questions are not in the PDF.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/letter/drafts/{n}/approve",
    tag = "pipeline",
    operation_id = "pipeline.letter_approve",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    responses(
        (status = 200, description = "The letter, with the version.", body = Letter),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`letter_flagged`; `letter_empty`; `draft_frozen`; \
            `letter_doesnt_fit`; `letter_pdf_check_failed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`letter_pdf_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn approve(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
) -> Result<Json<Letter>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    if row.content.0.paragraphs.is_empty() {
        return Err(Error::Conflict(
            "letter_empty",
            "The letter has no paragraphs left. Write it again.".to_owned(),
        ));
    }
    if !check::check(&row.content.0, &row.frame.0).is_empty() {
        return Err(Error::Conflict(
            "letter_flagged",
            "Some lines cannot be traced to your profile, or a question is waiting for your \
             answer. Fix or remove them first."
                .to_owned(),
        ));
    }
    let mut content = row.content.0.clone();
    if let Some(neighbours) = deps.neighbours.as_deref()
        && let Ok(Some(profile)) = neighbours.cv_profile(&state.db, account).await
    {
        content.contact = profile.contact.into();
    }
    // The PDF first (it takes a moment and needs no lock); then, in one
    // transaction, the approval and the version.
    let made = pdf::make(&content, &opp.company, &row.language)
        .await
        .map_err(|refusal| match refusal {
            Refusal::DoesntFit { .. } => Error::Conflict(
                "letter_doesnt_fit",
                "Doesn't fit on 1 page: remove a few sentences or write a shorter letter"
                    .to_owned(),
            ),
            Refusal::Checks(checks) => {
                // Counts only: the lines are the person's.
                tracing::warn!(
                    lines = checks.lines.missing.len(),
                    "a letter PDF failed its read-back"
                );
                Error::Conflict("letter_pdf_check_failed", checks.reasons().join("\n"))
            }
            Refusal::Unavailable => Error::Unavailable("letter_pdf_unavailable"),
        })?;
    let mut tx = state.db.begin().await?;
    let version = next_version(&mut tx, opp.id).await?;
    let approved = sqlx::query(
        "UPDATE pipeline.letter_drafts
            SET status = 'approved', version = $2, approved_at = $3, updated_at = $3,
                expires_at = NULL, content = $4, checks = '{}'::jsonb
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(row.id)
    .bind(version)
    .bind(now)
    .bind(sqlx::types::Json(&content))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if approved == 0 {
        return Err(Error::Conflict(
            "draft_frozen",
            "This letter was approved or put away, so it cannot change. Write it again.".to_owned(),
        ));
    }
    sqlx::query(
        "INSERT INTO pipeline.letter_versions
            (id, account_id, opportunity_id, draft_id, version, language, pdf, size_bytes,
             pages, checks, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(opp.id)
    .bind(row.id)
    .bind(version)
    .bind(&row.language)
    .bind(&made.bytes)
    .bind(
        i32::try_from(made.bytes.len())
            .map_err(|_| Error::Unavailable("letter_pdf_unavailable"))?,
    )
    .bind(i16::try_from(made.pages).map_err(|_| Error::Unavailable("letter_pdf_unavailable"))?)
    .bind(sqlx::types::Json(&made.checks))
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(version, pages = made.pages, "a cover letter approved");
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// Put a draft away.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/letter/drafts/{n}/discard",
    tag = "pipeline",
    operation_id = "pipeline.letter_discard",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    responses(
        (status = 200, description = "The letter.", body = Letter),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn discard(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
) -> Result<Json<Letter>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    sqlx::query(
        "UPDATE pipeline.letter_drafts SET status = 'discarded', updated_at = $2
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(row.id)
    .bind(now)
    .execute(&state.db)
    .await?;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

#[derive(sqlx::FromRow)]
struct PdfRow {
    pdf: Vec<u8>,
    name: Option<String>,
    company: String,
    language: String,
}

/// The PDF of a letter version.
///
/// `Content-Type: application/pdf`, shown inline, named
/// `<Name> Cover letter - <Company>.pdf`. `?download=1` serves it as an
/// attachment and sets the version's `downloaded_at`. Never cached, never
/// sniffed. Another account's version is not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/letter/versions/{version}/pdf",
    tag = "pipeline",
    operation_id = "pipeline.letter_pdf",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("version" = u32, Path, description = "The version."), PdfQuery),
    responses(
        (status = 200, description = "The PDF.", content_type = "application/pdf",
            body = PdfFile),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn pdf_file(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path((id, version)): Path<(Uuid, u32)>,
    Query(query): Query<PdfQuery>,
) -> Result<Response> {
    let account = principal.account_id;
    let version = i32::try_from(version).map_err(|_| Error::NotFound)?;
    let row: PdfRow = sqlx::query_as(
        "SELECT v.pdf, d.content -> 'contact' ->> 'name' AS name, o.company, v.language
           FROM pipeline.letter_versions v
           JOIN pipeline.letter_drafts d ON d.id = v.draft_id
           JOIN pipeline.opportunities o ON o.id = v.opportunity_id
          WHERE v.opportunity_id = $1 AND v.account_id = $2 AND v.version = $3",
    )
    .bind(id)
    .bind(account)
    .bind(version)
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;
    let download = matches!(query.download.as_deref(), Some("1" | "true"));
    if download {
        sqlx::query(
            "UPDATE pipeline.letter_versions SET downloaded_at = $4
              WHERE opportunity_id = $1 AND account_id = $2 AND version = $3",
        )
        .bind(id)
        .bind(account)
        .bind(version)
        .bind(state.clock.now())
        .execute(&state.db)
        .await?;
    }
    let noun = match lang_of(&row.language) {
        "pt" => "Carta",
        "es" => "Carta",
        "fr" => "Lettre",
        _ => "Cover letter",
    };
    let who = row
        .name
        .as_deref()
        .map(file_stem)
        .filter(|n| !n.is_empty())
        .map_or_else(|| noun.to_owned(), |name| format!("{name} {noun}"));
    let company = file_stem(&row.company);
    let stem = if company.is_empty() {
        who
    } else {
        format!("{who} - {company}")
    };
    let disposition = HeaderValue::from_str(&disposition(
        if download { "attachment" } else { "inline" },
        &stem,
    ))
    .map_err(|_| Error::Unavailable("letter_pdf_unavailable"))?;
    let length = row.pdf.len();
    let mut response = Response::new(axum::body::Body::from(row.pdf));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    Ok(response)
}

#[cfg(test)]
mod tests;
