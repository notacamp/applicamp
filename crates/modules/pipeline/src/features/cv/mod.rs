//! The tailored CV, part A: draft, fact check, review and approve (#37).
//!
//! For an opportunity at Shortlisted or later (`/api/v1/opportunities/{id}/cv…`):
//!
//! - **Draft** (one model call, on a click): the model is shown the posting,
//!   what the job wants, the company facts the person has not removed and the
//!   profile cut into sources with ids ([`model`], [`prompt`]), and answers a
//!   headline, a summary, positions with bullets and skills, each line citing
//!   the sources it comes from. It never sees a name or how to reach the
//!   person: the server prints those itself.
//! - **Fact check** (no model, [`check`]): each line may say only what the
//!   sources it cites say (the claims that matter: numbers, tools, employers,
//!   acronyms, places the profile does not name); a bullet cites its own
//!   position only; a company fact may be cited in the summary only; every
//!   skill must be on the profile. A line that fails is blocked, with the
//!   reason, and **Approve stays off** while any is.
//! - **The app fixes first** ([`fix`], #73): one more call rewrites each
//!   flagged line to what its sources say, or leaves one question with the
//!   fact "Yes, keep it" would keep. Only the person's answer reaches the
//!   profile.
//! - **A PDF at once** ([`preview`], #73): every draft is rendered as a PDF as
//!   soon as it is made and after every change, so the person sees what they
//!   approve.
//! - **Review**: answer a question ("Yes, keep it" keeps the proposed fact on
//!   the position as a confirmed assumption), remove a line, edit it (checked
//!   again against the same sources), say "it's true" in their own words, put
//!   back what was left out as the profile has it. Approve freezes the draft
//!   as a version with its PDF (part B, #38).
//!
//! Every model call starts with a click, or with the preparation of a job the
//! person shortlisted ([`crate::features::prepare`]); nothing runs on a timer
//! but the sweep of drafts nobody approved. At most [`CAP_PER_OPPORTUNITY`]
//! drafts per opportunity and [`CAP_PER_DAY`] per account per UTC day, all
//! through the AI budget.

pub mod check;
pub mod fix;
pub mod fix_prompt;
pub mod judge;
pub mod letter;
pub mod model;
pub mod pdf;
pub mod preview;
pub mod prompt;
#[cfg(test)]
pub(crate) mod tests;
pub mod versions;
pub mod view;

use applicamp_ai::Role;
use applicamp_kernel::{
    AppState, Error, Job, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{
    Extension, Json,
    extract::{Path, rejection::JsonRejection},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use self::{
    check::{Checks, Known},
    judge::{MAX_BULLET, MAX_HEADLINE, MAX_SKILL, MAX_SUMMARY, RawCv},
    model::{CompanyFact, Content, Frame, Line, Place, Role as CvRole, Source, SourceKind, short},
    pdf::{Length, Template, readback::PdfChecks},
    versions::LayoutRequest,
    view::{CvDraftView, CvVersion, DraftParts, DraftStatus},
};
use crate::{
    PipelineDeps, companies,
    features::brief::{self, BriefWant},
    ports::Neighbours,
};

/// Drafts one opportunity can have, ever (until they are swept).
pub const CAP_PER_OPPORTUNITY: i64 = 10;
/// Drafts one account can start in a UTC day.
pub const CAP_PER_DAY: i64 = 20;
/// About what a draft costs, in euros: the draft's call, and at worst the
/// app's fix round after it (one more call of up to 3,000 tokens).
pub const DRAFT_EUR: f64 = 0.004;
/// How long a draft nobody approves is kept after its last change.
pub const LIFETIME: TimeDelta = TimeDelta::days(7);
/// How long a CV just approved can be put back as a draft (Undo); the cover
/// letter a preparation writes next waits as long.
pub const UNDO_WINDOW: TimeDelta = TimeDelta::seconds(15);
/// Longest fact the person adds from a CV, in characters.
const MAX_FACT: usize = 300;
/// What a fact added from a CV can be about (Grill me's areas).
const AREAS: [&str; 7] = [
    "context",
    "your_part",
    "stack_scale",
    "results",
    "hard_problems",
    "people",
    "stories",
];

// ---- views ----------------------------------------------------------------

/// Why a draft cannot be made now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Blocked {
    /// The opportunity is still Found: shortlist it first.
    StageTooEarly,
    /// This opportunity has its drafts.
    PerOpportunityCap,
    /// The account's drafts for today.
    DailyCap,
    /// No AI model is set up.
    AiUnavailable,
}

/// Whether "Draft the CV" is on.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct CanDraft {
    pub allowed: bool,
    pub reason: Option<Blocked>,
}

/// The caps and the spend.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct CvCaps {
    /// Drafts this opportunity has had.
    pub drafts_here: u32,
    pub drafts_here_cap: u32,
    /// Drafts the account started today (UTC).
    pub drafts_today: u32,
    pub drafts_today_cap: u32,
    pub today_eur: f64,
    pub day_limit_eur: Option<f64>,
    pub month_eur: f64,
    pub month_limit_eur: Option<f64>,
}

/// What the app starts by itself now that the CV is approved.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvNext {
    /// `letter`: the cover letter and the job's answers.
    pub step: String,
    /// About what it costs, in euros.
    pub eur: f64,
    /// When it starts: Undo stops it until then.
    pub at: DateTime<Utc>,
}

/// The tailored CV of an opportunity.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Cv {
    pub opportunity_id: Uuid,
    /// The latest draft that was not discarded (approved or not); none before
    /// the first.
    pub draft: Option<CvDraftView>,
    /// The PDF versions, newest first.
    pub versions: Vec<CvVersion>,
    pub can_draft: CanDraft,
    pub caps: CvCaps,
    /// About what one draft costs, in euros.
    pub draft_eur: f64,
    /// What the app starts next by itself, when it does.
    pub next: Option<CvNext>,
}

/// What to make a draft of.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CvDraftRequest {
    /// A language tag such as `pt-PT`; the posting's when it says, else
    /// English.
    #[serde(default)]
    pub language: Option<String>,
}

/// A change to one line: its new text, or its removal.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LineChange {
    /// The new text. It is checked against the same sources.
    #[serde(default)]
    pub text: Option<String>,
    /// Remove the line (or the skill).
    #[serde(default)]
    pub remove: Option<bool>,
}

/// "Yes, keep it", or "It's true" in the person's words.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmRequest {
    /// Keep the fact the app proposed for this line (its `waiting` item's
    /// `proposal`) as a confirmed assumption on the position.
    #[serde(default)]
    pub proposal: Option<bool>,
    /// In their own words, up to 300 characters. Not needed for a skill or
    /// with `proposal`.
    #[serde(default)]
    pub text: Option<String>,
    /// What it is about (Grill me's areas); `results` when left out.
    #[serde(default)]
    pub area: Option<String>,
}

/// What to put back: a position, or one of its lines.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PutBackRequest {
    /// A position left out (its place in the profile's `work`).
    #[serde(default)]
    pub position: Option<u32>,
    /// A line of the profile left out (its source id).
    #[serde(default)]
    pub source: Option<String>,
}

// ---- storage --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Opp {
    id: Uuid,
    title: String,
    company: String,
    description: Option<String>,
    stage: String,
    working_languages: Option<Vec<String>>,
}

async fn load(db: &PgPool, account: Uuid, id: Uuid) -> Result<Opp> {
    sqlx::query_as(
        "SELECT id, title, company, description, stage, working_languages
           FROM pipeline.opportunities WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    n: i32,
    language: String,
    status: String,
    version: Option<i32>,
    content: sqlx::types::Json<Content>,
    frame: sqlx::types::Json<Frame>,
    checks: sqlx::types::Json<Checks>,
    created_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    /// The preview's PDF is there (its bytes are read only to serve it).
    preview_ready: bool,
    preview_template: Option<String>,
    preview_length: Option<i16>,
    preview_pages: Option<i16>,
    preview_checks: Option<sqlx::types::Json<PdfChecks>>,
    preview_error: Option<String>,
    preview_at: Option<DateTime<Utc>>,
    /// The fingerprint of the content the preview was made from.
    preview_of: Option<Vec<u8>>,
}

/// A select of a draft's columns, with `$tail` after the table.
macro_rules! select_draft {
    ($tail:literal) => {
        concat!(
            "SELECT id, n, language, status, version, content, frame, checks, created_at, \
             approved_at, expires_at, preview_pdf IS NOT NULL AS preview_ready, \
             preview_template, preview_length, preview_pages, preview_checks, preview_error, \
             preview_at, preview_of FROM pipeline.cv_drafts ",
            $tail
        )
    };
}

fn counted(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
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

/// The newest approved draft of `opp`.
async fn latest_approved(db: &PgPool, account: Uuid, opp: Uuid) -> Result<Option<Row>> {
    Ok(sqlx::query_as(select_draft!(
        "WHERE opportunity_id = $1 AND account_id = $2 AND status = 'approved'
          ORDER BY n DESC LIMIT 1"
    ))
    .bind(opp)
    .bind(account)
    .fetch_optional(db)
    .await?)
}

async fn wants_of(db: &PgPool, opp: Uuid) -> Result<Vec<BriefWant>> {
    let wants: Option<sqlx::types::Json<Vec<BriefWant>>> =
        sqlx::query_scalar("SELECT wants FROM pipeline.job_briefs WHERE opportunity_id = $1")
            .bind(opp)
            .fetch_optional(db)
            .await?;
    Ok(wants.map(|w| w.0).unwrap_or_default())
}

async fn drafts_here(db: &PgPool, opp: Uuid) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM pipeline.cv_drafts WHERE opportunity_id = $1")
            .bind(opp)
            .fetch_one(db)
            .await?,
    )
}

async fn drafts_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.cv_drafts WHERE account_id = $1 AND created_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

fn gateway_ready(deps: &PipelineDeps) -> bool {
    deps.ai
        .as_ref()
        .is_some_and(|ai| ai.is_configured(Role::Complete))
}

/// Whether a draft can be made now, and why not.
fn can_draft(stage: &str, here: i64, today: i64, ai: bool) -> CanDraft {
    let reason = if stage == "found" {
        Some(Blocked::StageTooEarly)
    } else if !ai {
        Some(Blocked::AiUnavailable)
    } else if here >= CAP_PER_OPPORTUNITY {
        Some(Blocked::PerOpportunityCap)
    } else if today >= CAP_PER_DAY {
        Some(Blocked::DailyCap)
    } else {
        None
    };
    CanDraft {
        allowed: reason.is_none(),
        reason,
    }
}

fn draft_view(row: &Row, wants: &[BriefWant], opp: &Opp) -> CvDraftView {
    let mut view = view::draft_view(
        &DraftParts {
            id: row.id,
            n: counted(i64::from(row.n)),
            status: DraftStatus::parse(&row.status),
            version: row.version.map(|v| counted(i64::from(v))),
            language: &row.language,
            created_at: row.created_at,
            approved_at: row.approved_at,
            expires_at: row.expires_at,
            content: &row.content.0,
            frame: &row.frame.0,
            checks: &row.checks.0,
        },
        wants,
        &opp.title,
        opp.description.as_deref().unwrap_or_default(),
    );
    view.preview = preview::view(row);
    view
}

/// What the check allows a draft of `opp` to say without citing it: the
/// posting's words, the places the profile names.
fn known_of(opp: &Opp, frame: &Frame, content: &Content) -> Known {
    let posting = format!(
        "{}\n{}",
        opp.title,
        opp.description.as_deref().unwrap_or_default()
    );
    Known::new(frame, &content.contact, &posting)
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    n: i32,
    version: i32,
    language: String,
    approved_at: Option<DateTime<Utc>>,
    template: String,
    length: i16,
    pages: i16,
    size_bytes: i32,
    created_at: DateTime<Utc>,
    downloaded_at: Option<DateTime<Utc>>,
    checks: sqlx::types::Json<pdf::readback::PdfChecks>,
}

/// The PDF versions of `opp`, newest first (the PDF itself is not read).
async fn versions_of(db: &PgPool, account: Uuid, opp: Uuid) -> Result<Vec<CvVersion>> {
    let rows: Vec<VersionRow> = sqlx::query_as(
        "SELECT d.n, v.version, v.language, d.approved_at, v.template, v.length, v.pages,
                v.size_bytes, v.created_at, v.downloaded_at, v.checks
           FROM pipeline.cv_versions v JOIN pipeline.cv_drafts d ON d.id = v.draft_id
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
        .map(|r| CvVersion {
            n: counted(i64::from(r.n)),
            version: counted(i64::from(r.version)),
            language: r.language,
            approved_at: r.approved_at.unwrap_or(r.created_at),
            template: Template::parse(&r.template).unwrap_or_default(),
            length: u8::try_from(r.length).unwrap_or(1),
            pages: counted(i64::from(r.pages)),
            size_bytes: u64::try_from(r.size_bytes).unwrap_or(0),
            created_at: r.created_at,
            downloaded_at: r.downloaded_at,
            current: Some(r.version) == newest,
            checks: r.checks.0,
        })
        .collect())
}

/// The tailored CV of `opp`, as it is now. Never calls the model.
async fn build(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: &Opp,
    now: DateTime<Utc>,
) -> Result<Cv> {
    let wants = wants_of(db, opp.id).await?;
    let draft = latest(db, account, opp.id, now)
        .await?
        .map(|row| draft_view(&row, &wants, opp));
    let versions = versions_of(db, account, opp.id).await?;
    // An approved draft is shown as its newest PDF's version; one approved
    // before PDFs were made has none yet, and offers to make it. Just
    // approved, it can be undone for a moment.
    let draft = draft.map(|mut draft| {
        if draft.status == DraftStatus::Approved {
            match versions.iter().find(|v| v.n == draft.n) {
                Some(newest) => draft.version = Some(newest.version),
                None => draft.needs_version = true,
            }
            draft.undo_until = draft
                .approved_at
                .map(|at| at + UNDO_WINDOW)
                .filter(|until| *until > now);
        }
        draft
    });
    let next: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT updated_at FROM pipeline.preparations
          WHERE opportunity_id = $1 AND account_id = $2 AND state = 'queued' AND step = 'letter'",
    )
    .bind(opp.id)
    .bind(account)
    .fetch_optional(db)
    .await?;
    let here = drafts_here(db, opp.id).await?;
    let today = drafts_today(db, account, now).await?;
    let spend = match &deps.ai {
        Some(ai) => ai.spend(db, account, now).await?,
        None => applicamp_ai::Spend::default(),
    };
    Ok(Cv {
        opportunity_id: opp.id,
        draft,
        versions,
        can_draft: can_draft(&opp.stage, here, today, gateway_ready(deps)),
        caps: CvCaps {
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
        next: next.map(|(queued,)| CvNext {
            step: "letter".to_owned(),
            eur: letter::DRAFT_EUR,
            at: queued + UNDO_WINDOW,
        }),
    })
}

fn neighbours(deps: &PipelineDeps) -> Result<&dyn Neighbours> {
    deps.neighbours
        .as_deref()
        .ok_or(Error::Unavailable("neighbours_unavailable"))
}

/// The language a draft is written in: the one asked for, else the posting's
/// first working language, else English.
fn language_of(asked: Option<&str>, opp: &Opp) -> String {
    brief::prompt::language(asked)
        .or_else(|| {
            opp.working_languages
                .as_ref()
                .and_then(|l| l.first())
                .and_then(|l| brief::prompt::language(Some(l)))
        })
        .unwrap_or_else(|| "en".to_owned())
}

// ---- the sweep ------------------------------------------------------------

/// Delete the drafts nobody approved whose time is up. Enqueued with every
/// change to a draft, to run when it expires; idempotent. Approved versions
/// are never deleted by it.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExpireCvDrafts {}

impl Job for ExpireCvDrafts {
    const NAME: &'static str = "pipeline.expire_cv_drafts";

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deleted = sqlx::query(
            "DELETE FROM pipeline.cv_drafts WHERE status <> 'approved' AND expires_at <= now()",
        )
        .execute(&ctx.db)
        .await?
        .rows_affected()
            + sqlx::query(
                "DELETE FROM pipeline.letter_drafts
                  WHERE status <> 'approved' AND expires_at <= now()",
            )
            .execute(&ctx.db)
            .await?
            .rows_affected();
        if deleted > 0 {
            tracing::info!(deleted, "expired CV and letter drafts deleted");
        }
        Ok(())
    }
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
        "UPDATE pipeline.cv_drafts
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
            "This draft was approved or put away, so it cannot change. Make the CV again."
                .to_owned(),
        ));
    }
    Ok((opp, row))
}

// ---- lines ----------------------------------------------------------------

/// Where a line is in the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Headline,
    Summary,
    Bullet(usize, usize),
    Skill(usize),
}

fn locate(content: &Content, id: &str) -> Option<Slot> {
    if content.headline.as_ref().is_some_and(|l| l.id == id) {
        return Some(Slot::Headline);
    }
    if content.summary.as_ref().is_some_and(|l| l.id == id) {
        return Some(Slot::Summary);
    }
    for (r, role) in content.experience.iter().enumerate() {
        if let Some(b) = role.bullets.iter().position(|l| l.id == id) {
            return Some(Slot::Bullet(r, b));
        }
    }
    content
        .skills
        .iter()
        .position(|s| s.id == id)
        .map(Slot::Skill)
}

fn line_mut(content: &mut Content, slot: Slot) -> Option<&mut Line> {
    match slot {
        Slot::Headline => content.headline.as_mut(),
        Slot::Summary => content.summary.as_mut(),
        Slot::Bullet(r, b) => content.experience.get_mut(r)?.bullets.get_mut(b),
        Slot::Skill(_) => None,
    }
}

fn invalid_text(most: usize) -> Error {
    Error::BadRequest(
        "invalid_text",
        format!("Write between 1 and {most} characters."),
    )
}

/// The checks of `content` after a change, saved with it, and its preview
/// made again in the layout the person sees.
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
) -> Result<Json<Cv>> {
    let checks = check::check(content, frame, &known_of(opp, frame, content));
    save(&state.db, row, content, frame, &checks, now).await?;
    let start = preview::layout_of(row).unwrap_or((Template::Clean, Length::One));
    preview::refresh(
        &state.db,
        opp,
        row.id,
        content,
        frame,
        &row.language,
        start,
        now,
    )
    .await;
    Ok(Json(build(deps, &state.db, account, opp, now).await?))
}

fn json_body<T>(
    body: std::result::Result<Json<T>, JsonRejection>,
    code: &'static str,
) -> Result<T> {
    body.map(|Json(b)| b)
        .map_err(|rejection| Error::BadRequest(code, rejection.body_text()))
}

// ---- endpoints ------------------------------------------------------------

/// The tailored CV of an opportunity.
///
/// The latest draft with each line's sources, marks and reasons, what was
/// left out, the job's keywords against it, the approved versions, the caps
/// and the spend. **Never calls the model.** Another account's opportunity is
/// not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}/cv",
    tag = "pipeline",
    operation_id = "pipeline.cv",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    responses(
        (status = 200, description = "The CV.", body = Cv),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<Cv>> {
    let account = principal.account_id;
    let opp = load(&state.db, account, id).await?;
    Ok(Json(
        build(&deps, &state.db, account, &opp, state.clock.now()).await?,
    ))
}

/// Draft a CV for this job.
///
/// **One model call.** From the posting, what the job wants, the company
/// facts not removed and the profile; each line cites the profile sources it
/// comes from, and the server then checks that it says nothing they do not.
/// The model never sees a name or contact details. Needs the opportunity at
/// Shortlisted or later. When the job was never compared, the company is read
/// and the job compared first (up to two more calls, as Read does), so the
/// draft knows what the job wants; a company that cannot be read does not stop
/// the draft. At most 10 drafts per opportunity and 20 per account per UTC
/// day; nothing retries past a limit. A new draft puts the earlier unapproved
/// one away.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts",
    tag = "pipeline",
    operation_id = "pipeline.cv_draft",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    request_body = CvDraftRequest,
    responses(
        (status = 200, description = "The CV, with the new draft.", body = Cv),
        (status = 400, description = "`invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`stage_too_early`; `profile_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`cv_drafts_per_opportunity_cap`; `cv_drafts_daily_cap`",
            body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn create(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    body: Option<Json<CvDraftRequest>>,
) -> Result<Json<Cv>> {
    let request = body.map(|Json(b)| b).unwrap_or_default();
    let account = principal.account_id;
    let now = state.clock.now();
    let db = &state.db;
    let opp = load(db, account, id).await?;
    if opp.stage != "found" && !brief::was_compared(db, opp.id).await? {
        // Read first, so the draft knows what the job wants (#70). A read that
        // cannot happen leaves the draft to go on without it.
        let language = request.language.as_deref();
        if let Err(e) =
            brief::read_and_compare(&deps, db, account, opp.id, language, true, now).await
        {
            if matches!(
                e.code(),
                "ai_daily_limit_reached" | "ai_budget_reached" | "ai_unavailable"
            ) {
                return Err(e);
            }
            tracing::info!(error = e.code(), "a CV is drafted without reading first");
        }
    }
    draft(&deps, db, account, opp.id, request.language.as_deref(), now).await?;
    let opp = load(db, account, id).await?;
    Ok(Json(build(&deps, db, account, &opp, now).await?))
}

/// Make a draft of `id`'s CV (**one model call**), with the same checks and
/// caps as "Draft the CV": the endpoint's and the preparation's one way in.
pub(crate) async fn draft(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    id: Uuid,
    language: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let opp = load(db, account, id).await?;
    if opp.stage == "found" {
        return Err(Error::Conflict(
            "stage_too_early",
            "Shortlist this job first: a CV is made for the jobs you are going after.".to_owned(),
        ));
    }
    let gateway = match &deps.ai {
        Some(gateway) if gateway.is_configured(Role::Complete) => gateway,
        _ => return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    };
    let neighbours = neighbours(deps)?;
    if drafts_here(db, opp.id).await? >= CAP_PER_OPPORTUNITY {
        return Err(Error::TooManyRequests(
            "cv_drafts_per_opportunity_cap",
            format!("You can make {CAP_PER_OPPORTUNITY} drafts for one job. Approve or edit one."),
        ));
    }
    if drafts_today(db, account, now).await? >= CAP_PER_DAY {
        return Err(Error::TooManyRequests(
            "cv_drafts_daily_cap",
            format!("You can make {CAP_PER_DAY} drafts a day. Drafting continues tomorrow."),
        ));
    }
    let profile = neighbours
        .cv_profile(db, account)
        .await?
        .filter(|p| !p.positions.is_empty())
        .ok_or_else(|| {
            Error::Conflict(
                "profile_needed",
                "Add a position to your profile first: the CV is made from it.".to_owned(),
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
    let language = language_of(language, &opp);
    let user = prompt::message(
        &opp.title,
        &opp.company,
        opp.description.as_deref().unwrap_or_default(),
        &wants,
        &frame,
        &language,
    );
    let raw: RawCv = brief::ask(
        gateway,
        db,
        account,
        now,
        "pipeline_tailored_cv",
        &prompt::schema(),
        prompt::INSTRUCTIONS,
        &user,
    )
    .await?;
    let mut content =
        judge::judge(raw, &frame, &profile).ok_or(Error::Unavailable(applicamp_ai::AI_FAILED))?;
    let known = known_of(&opp, &frame, &content);
    let mut checks = check::check(&content, &frame, &known);
    // The app fixes what it can before the person is asked (#73).
    let fixed = fix::fix(
        gateway,
        db,
        account,
        now,
        &mut content,
        &frame,
        &known,
        &checks,
        &language,
    )
    .await;
    if fixed.rewritten > 0 {
        checks = check::check(&content, &frame, &known);
    }

    let expires_at = now + LIFETIME;
    let id = Uuid::now_v7();
    let mut tx = db.begin().await?;
    let n: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(n), 0) + 1 FROM pipeline.cv_drafts WHERE opportunity_id = $1",
    )
    .bind(opp.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET status = 'discarded', updated_at = $2, preview_pdf = NULL
          WHERE opportunity_id = $1 AND status = 'draft'",
    )
    .bind(opp.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO pipeline.cv_drafts
            (id, account_id, opportunity_id, n, language, status, content, frame, checks,
             created_at, updated_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, 'draft', $6, $7, $8, $9, $9, $10)",
    )
    .bind(id)
    .bind(account)
    .bind(opp.id)
    .bind(n)
    .bind(&language)
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
        positions = content.experience.len(),
        flagged = checks.len(),
        fixed = fixed.rewritten,
        asked = fixed.asked,
        "a CV drafted"
    );
    // The PDF at once: the person sees what they will approve.
    preview::refresh(
        db,
        &opp,
        id,
        &content,
        &frame,
        &language,
        (Template::Clean, Length::One),
        now,
    )
    .await;
    Ok(())
}

/// Change one line of a draft.
///
/// `{text}` edits it: the new text is checked against the **same sources**
/// (an edited line that is not a summary drops its company-fact citations,
/// and a bullet drops another position's), and the app's question about it,
/// if any, goes. The preview is made again.
/// `{remove: true}` removes the line (a position left with no line goes to
/// "left out"), or the skill. No model call. Only a draft can change.
#[utoipa::path(
    patch,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/lines/{line_id}",
    tag = "pipeline",
    operation_id = "pipeline.cv_line",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."),
        ("line_id" = String, Path, description = "The line (or skill) id.")),
    request_body = LineChange,
    responses(
        (status = 200, description = "The CV, after the change.", body = Cv),
        (status = 400, description = "`invalid_change`; `invalid_text`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such opportunity, draft or line",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn change_line(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n, line)): Path<(Uuid, u32, String)>,
    body: std::result::Result<Json<LineChange>, JsonRejection>,
) -> Result<Json<Cv>> {
    let change = json_body(body, "invalid_change")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let mut content = row.content.0.clone();
    let slot = locate(&content, &line).ok_or(Error::NotFound)?;
    match (change.text, change.remove) {
        (Some(text), None | Some(false)) => match slot {
            Slot::Skill(i) => {
                let name =
                    judge::edited(&text, MAX_SKILL).ok_or_else(|| invalid_text(MAX_SKILL))?;
                if let Some(skill) = content.skills.get_mut(i) {
                    skill.name = name;
                }
            }
            Slot::Headline | Slot::Summary | Slot::Bullet(..) => {
                let most = match slot {
                    Slot::Headline => MAX_HEADLINE,
                    Slot::Summary => MAX_SUMMARY,
                    _ => MAX_BULLET,
                };
                let text = judge::edited(&text, most).ok_or_else(|| invalid_text(most))?;
                let frame = &row.frame.0;
                let place = content.place_of(&line);
                if let Some(target) = line_mut(&mut content, slot) {
                    target.text = text;
                    target.edited = true;
                    target.fixed_by_app = false;
                    target.ask = None;
                    if slot != Slot::Summary {
                        // The person rewrote it: a company fact is not an
                        // achievement of theirs, and another position's line
                        // is not this one's, so they no longer back it.
                        target.sources.retain(|id| {
                            frame.get(id).is_none_or(|s| {
                                s.kind != SourceKind::Company
                                    && !matches!(
                                        (place, s.position),
                                        (Some(Place::Bullet(own)), Some(at)) if at != own
                                    )
                            })
                        });
                    }
                }
            }
        },
        (None, Some(true)) => match slot {
            Slot::Headline => content.headline = None,
            Slot::Summary => content.summary = None,
            Slot::Skill(i) => {
                content.skills.remove(i);
            }
            Slot::Bullet(r, b) => {
                if let Some(role) = content.experience.get_mut(r) {
                    role.bullets.remove(b);
                    if role.bullets.is_empty() {
                        content.experience.remove(r);
                    }
                }
            }
        },
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

/// "Yes, keep it", or "It's true" in the person's words.
///
/// `{proposal: true}` keeps the fact the app proposed for a flagged line
/// (its question's, else the line's own words) on the line's position as a
/// **confirmed assumption**, based on this CV draft; refused
/// (`cannot_keep`) for a company fact used as an achievement, another job's
/// line, or one longer than a fact (its `waiting` item says which). `{text}`
/// keeps the person's own words as a fact (its `quote` is the text; `area`
/// defaults to `results`), as Grill me's Keep does. Either way the line is
/// checked again with that fact as a source (and none another position's or
/// a company's), and the preview is made again. For a skill it adds the
/// skill to the profile instead. A summary or headline puts the fact on the
/// most recent position. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/lines/{line_id}/confirm",
    tag = "pipeline",
    operation_id = "pipeline.cv_confirm",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft."),
        ("line_id" = String, Path, description = "The line (or skill) id.")),
    request_body = ConfirmRequest,
    responses(
        (status = 200, description = "The CV, after the line was checked again.", body = Cv),
        (status = 400, description = "`invalid_fact`; `invalid_area`; `no_position`",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`; `profile_changed`; `cannot_keep`; \
            `not_flagged`", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn confirm_line(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n, line)): Path<(Uuid, u32, String)>,
    body: std::result::Result<Json<ConfirmRequest>, JsonRejection>,
) -> Result<Json<Cv>> {
    let request = json_body(body, "invalid_fact")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let neighbours = neighbours(&deps)?;
    let mut content = row.content.0.clone();
    let mut frame = row.frame.0.clone();
    let slot = locate(&content, &line).ok_or(Error::NotFound)?;
    if let Slot::Skill(i) = slot {
        let name = content
            .skills
            .get(i)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        neighbours.add_skill(&state, account, &name).await?;
        if !check::skill_on_profile(&name, &frame) {
            frame.skills.push(name);
        }
    } else if request.proposal == Some(true) {
        let known = known_of(&opp, &frame, &content);
        let reasons = check::check(&content, &frame, &known)
            .remove(&line)
            .ok_or_else(|| {
                Error::Conflict(
                    "not_flagged",
                    "This line traces to your profile already.".to_owned(),
                )
            })?;
        let place = content.place_of(&line).ok_or(Error::NotFound)?;
        if let Some(why) = check::cannot_keep(&reasons, place, &frame) {
            return Err(Error::Conflict(
                "cannot_keep",
                match why {
                    check::CannotKeep::OtherPosition => {
                        "This line is about another job on your profile. Change it or remove it."
                    }
                    check::CannotKeep::Company => {
                        "A fact about the company can't be your achievement. Change the line or \
                         remove it."
                    }
                    check::CannotKeep::TooLong => {
                        "This line is too long to keep as a fact. Change it or remove it."
                    }
                }
                .to_owned(),
            ));
        }
        let target = content
            .lines()
            .map(|(l, _)| l)
            .find(|l| l.id == line)
            .cloned()
            .ok_or(Error::NotFound)?;
        let proposal = target
            .ask
            .as_ref()
            .and_then(|a| a.proposal.clone())
            .filter(|p| fix::backs(p, &target, place, &frame, &known, frame.latest))
            .unwrap_or_else(|| target.text.clone());
        if proposal.chars().count() > MAX_FACT {
            return Err(Error::Conflict(
                "cannot_keep",
                format!("Change the line to {MAX_FACT} characters or fewer to keep it."),
            ));
        }
        let position = content.position_of(&line, frame.latest).ok_or_else(|| {
            Error::BadRequest(
                "no_position",
                "The profile has no position to put it on.".to_owned(),
            )
        })?;
        let info = frame.position(position).cloned().ok_or(Error::NotFound)?;
        let fact = neighbours
            .keep_assumption(
                &state,
                account,
                position,
                info.title.as_deref(),
                info.company.as_deref(),
                &proposal,
                &format!("The CV draft for {}", opp.company),
            )
            .await?;
        let source = format!("w{position}.f{}", short(&fact));
        let order = frame
            .of_position(position)
            .filter_map(|s| s.order)
            .max()
            .map_or(0, |o| o + 1);
        let usable = check::usable_sources(&target, place, &frame);
        frame.sources.push(Source {
            id: source.clone(),
            kind: SourceKind::Fact,
            text: proposal,
            position: Some(position),
            order: Some(order),
            url: None,
        });
        if let Some(changed) = line_mut(&mut content, slot) {
            changed.sources = usable;
            changed.sources.push(source);
            changed.ask = None;
        }
    } else {
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
        let position = content.position_of(&line, frame.latest).ok_or_else(|| {
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
        let place = content.place_of(&line);
        if let Some(target) = line_mut(&mut content, slot) {
            target.ask = None;
            if let Some(place) = place {
                // A company fact or another position's line no longer backs it.
                target.sources = check::usable_sources(target, place, &frame);
            }
            if !target.sources.contains(&source) {
                target.sources.push(source);
            }
        }
    }
    recheck_and_save(&state, &deps, account, &opp, &row, &content, &frame, now).await
}

/// Put back what the draft left out, as the profile has it.
///
/// `{position}` puts a position left out back with up to three of its lines
/// (highlights, else kept facts, else its summary), word for word; `{source}`
/// puts one left-out line back on its position. Each cites its own source, so
/// it passes the check. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/put-back",
    tag = "pipeline",
    operation_id = "pipeline.cv_put_back",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    request_body = PutBackRequest,
    responses(
        (status = 200, description = "The CV, with it put back.", body = Cv),
        (status = 400, description = "`invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such draft, position or line",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`; `already_on_cv`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn put_back(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
    body: std::result::Result<Json<PutBackRequest>, JsonRejection>,
) -> Result<Json<Cv>> {
    let request = json_body(body, "invalid_request")?;
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let frame = &row.frame.0;
    let mut content = row.content.0.clone();
    match (request.position, request.source) {
        (Some(position), None) => {
            let position = usize::try_from(position).map_err(|_| Error::NotFound)?;
            frame.position(position).ok_or(Error::NotFound)?;
            if content.experience.iter().any(|r| r.position == position) {
                return Err(Error::Conflict(
                    "already_on_cv",
                    "That position is on the CV already.".to_owned(),
                ));
            }
            let pick = |kinds: &[SourceKind]| -> Vec<Source> {
                frame
                    .of_position(position)
                    .filter(|s| kinds.contains(&s.kind))
                    .take(3)
                    .cloned()
                    .collect()
            };
            let mut chosen = pick(&[SourceKind::Highlight]);
            if chosen.is_empty() {
                chosen = pick(&[SourceKind::Fact]);
            }
            if chosen.is_empty() {
                chosen = pick(&[SourceKind::PositionSummary])
                    .into_iter()
                    .filter(|s| s.text.chars().count() <= MAX_BULLET)
                    .collect();
            }
            let bullets: Vec<Line> = chosen
                .into_iter()
                .map(|s| Line {
                    id: content.new_line_id(),
                    text: s.text,
                    sources: vec![s.id],
                    edited: false,
                    ..Line::default()
                })
                .collect();
            let at = content
                .experience
                .iter()
                .position(|r| r.position > position)
                .unwrap_or(content.experience.len());
            content.experience.insert(at, CvRole { position, bullets });
        }
        (None, Some(source)) => {
            let found = frame.get(&source).ok_or(Error::NotFound)?;
            let (Some(position), true) = (
                found.position,
                matches!(
                    found.kind,
                    SourceKind::Highlight | SourceKind::Fact | SourceKind::PositionSummary
                ),
            ) else {
                return Err(Error::BadRequest(
                    "invalid_request",
                    "Only a line of a position can be put back.".to_owned(),
                ));
            };
            if content
                .lines()
                .any(|(l, _)| l.sources.len() == 1 && l.sources.first() == Some(&source))
            {
                return Err(Error::Conflict(
                    "already_on_cv",
                    "That line is on the CV already.".to_owned(),
                ));
            }
            let line = Line {
                id: content.new_line_id(),
                text: found.text.clone(),
                sources: vec![source],
                edited: false,
                ..Line::default()
            };
            match content
                .experience
                .iter_mut()
                .find(|r| r.position == position)
            {
                Some(role) => role.bullets.push(line),
                None => {
                    let at = content
                        .experience
                        .iter()
                        .position(|r| r.position > position)
                        .unwrap_or(content.experience.len());
                    content.experience.insert(
                        at,
                        CvRole {
                            position,
                            bullets: vec![line],
                        },
                    );
                }
            }
        }
        _ => {
            return Err(Error::BadRequest(
                "invalid_request",
                "Send either a `position` or a `source`.".to_owned(),
            ));
        }
    }
    recheck_and_save(&state, &deps, account, &opp, &row, &content, frame, now).await
}

/// Approve the draft.
///
/// Refused (409 `lines_flagged`) while any line or skill cannot be traced to
/// the profile. The draft is frozen, its text rendered as a PDF with the
/// `template` and `length` asked for (the preview's when left out, else
/// Clean on 1 page), the PDF read back out and compared with the CV, and the
/// PDF kept as the next version: all or nothing. The preview's PDF is kept
/// as it is when the draft has not changed since it was made, its layout is
/// the one asked for and its read-back passed. If it does not fit the length (`cv_doesnt_fit`) or the
/// read-back finds it lacking (`cv_pdf_check_failed`, with the reasons, one
/// per line), nothing is approved. Contact details are read from the profile
/// again. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/approve",
    tag = "pipeline",
    operation_id = "pipeline.cv_approve",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    request_body(content = LayoutRequest, description = "The look of the PDF; optional."),
    responses(
        (status = 200, description = "The CV, with the version.", body = Cv),
        (status = 400, description = "`invalid_layout`; `invalid_length`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`lines_flagged`; `draft_frozen`; `cv_doesnt_fit`; \
            `cv_pdf_check_failed`", body = Problem, content_type = "application/problem+json"),
        (status = 503, description = "`cv_pdf_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn approve(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
    body: Option<Json<LayoutRequest>>,
) -> Result<Json<Cv>> {
    let request = body.map(|Json(b)| b).unwrap_or_default();
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    let fallback = preview::layout_of(&row).unwrap_or((Template::Clean, Length::One));
    let (template, length) = versions::layout(&request, fallback)?;
    // Checked again now, not trusted from the last change.
    let known = known_of(&opp, &row.frame.0, &row.content.0);
    if !check::check(&row.content.0, &row.frame.0, &known).is_empty() {
        return Err(Error::Conflict(
            "lines_flagged",
            "Some lines cannot be traced to your profile. Fix or remove them first.".to_owned(),
        ));
    }
    let mut content = row.content.0.clone();
    if let Some(neighbours) = deps.neighbours.as_deref()
        && let Ok(Some(profile)) = neighbours.cv_profile(&state.db, account).await
    {
        content.contact = profile.contact.into();
    }
    // The PDF first (it takes a moment and needs no lock): the preview's when
    // it is this text in this layout; then, in one transaction, the approval
    // and the version.
    let kept = preview::reusable(&state.db, &row, &content, (template, length)).await?;
    let made = match kept {
        Some(made) => made,
        None => {
            versions::render(
                &state.db,
                &opp,
                &content,
                &row.frame.0,
                &row.language,
                template,
                length,
            )
            .await?
        }
    };
    let mut tx = state.db.begin().await?;
    let version = versions::next_version(&mut tx, opp.id).await?;
    let approved = sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET status = 'approved', version = $2, approved_at = $3, updated_at = $3,
                expires_at = NULL, content = $4, checks = '{}'::jsonb,
                preview_pdf = NULL, preview_template = NULL, preview_length = NULL,
                preview_pages = NULL, preview_checks = NULL, preview_error = NULL,
                preview_at = NULL, preview_of = NULL
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
            "This draft was approved or put away, so it cannot change. Make the CV again."
                .to_owned(),
        ));
    }
    versions::insert(
        &mut tx,
        account,
        opp.id,
        row.id,
        version,
        &row.language,
        template,
        length,
        &made,
        now,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(version, pages = made.pages, "a CV approved");
    crate::features::prepare::nudge(&deps, &state.db, account, opp.id, now).await;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}

/// Undo an approval, for a moment.
///
/// Within 15 seconds of approving, before the cover letter that comes next
/// starts, the draft becomes a draft again: its PDF version goes, the
/// letter waits for the CV again, and the preview is made again. Refused
/// after that (`too_late`), or when it is not approved (`not_approved`). No
/// model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/unapprove",
    tag = "pipeline",
    operation_id = "pipeline.cv_unapprove",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    responses(
        (status = 200, description = "The CV, with the draft back.", body = Cv),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`not_approved`; `too_late`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn unapprove(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
) -> Result<Json<Cv>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let db = &state.db;
    let opp = load(db, account, id).await?;
    let row = by_n(db, account, opp.id, n, now).await?;
    if DraftStatus::parse(&row.status) != DraftStatus::Approved {
        return Err(Error::Conflict(
            "not_approved",
            "This CV is not approved.".to_owned(),
        ));
    }
    let too_late = || {
        Error::Conflict(
            "too_late",
            "It's too late to undo. Make the CV again to change it.".to_owned(),
        )
    };
    let approved_at = row.approved_at.ok_or_else(too_late)?;
    if approved_at + UNDO_WINDOW <= now {
        return Err(too_late());
    }
    let mut tx = db.begin().await?;
    let preparing: Option<(String, String)> = sqlx::query_as(
        "SELECT state, step FROM pipeline.preparations WHERE opportunity_id = $1 FOR UPDATE",
    )
    .bind(opp.id)
    .fetch_optional(&mut *tx)
    .await?;
    let letters: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.letter_drafts
          WHERE opportunity_id = $1 AND created_at >= $2",
    )
    .bind(opp.id)
    .bind(approved_at)
    .fetch_one(&mut *tx)
    .await?;
    let later: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.cv_drafts WHERE opportunity_id = $1 AND n > $2",
    )
    .bind(opp.id)
    .bind(row.n)
    .fetch_one(&mut *tx)
    .await?;
    let writing = preparing
        .as_ref()
        .is_some_and(|(state, step)| state == "running" && step == "letter");
    if writing || letters > 0 || later > 0 {
        return Err(too_late());
    }
    // The letter the preparation would write next waits for the CV again.
    sqlx::query(
        "UPDATE pipeline.preparations
            SET state = 'waiting', waiting_on = 'cv_approval', updated_at = $2
          WHERE opportunity_id = $1 AND state = 'queued' AND step = 'letter'",
    )
    .bind(opp.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let layout: Option<(String, i16)> = sqlx::query_as(
        "SELECT template, length FROM pipeline.cv_versions
          WHERE draft_id = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(row.id)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM pipeline.cv_versions WHERE draft_id = $1")
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    let content = row.content.0.clone();
    let frame = row.frame.0.clone();
    let checks = check::check(&content, &frame, &known_of(&opp, &frame, &content));
    let expires_at = now + LIFETIME;
    sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET status = 'draft', version = NULL, approved_at = NULL, updated_at = $2,
                expires_at = $3, checks = $4
          WHERE id = $1 AND status = 'approved'",
    )
    .bind(row.id)
    .bind(now)
    .bind(expires_at)
    .bind(sqlx::types::Json(&checks))
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireCvDrafts {}, expires_at).await?;
    tx.commit().await?;
    tracing::info!("a CV approval undone");
    let start = layout
        .and_then(|(template, length)| {
            Some((
                Template::parse(&template)?,
                Length::from_pages(u8::try_from(length).ok()?)?,
            ))
        })
        .unwrap_or((Template::Clean, Length::One));
    preview::refresh(
        db,
        &opp,
        row.id,
        &content,
        &frame,
        &row.language,
        start,
        now,
    )
    .await;
    Ok(Json(build(&deps, db, account, &opp, now).await?))
}

/// Put a draft away.
///
/// An approved version cannot be discarded. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/cv/drafts/{n}/discard",
    tag = "pipeline",
    operation_id = "pipeline.cv_discard",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."),
        ("n" = u32, Path, description = "The draft.")),
    responses(
        (status = 200, description = "The CV.", body = Cv),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`draft_frozen`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn discard(
    axum::extract::State(state): axum::extract::State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path((id, n)): Path<(Uuid, u32)>,
) -> Result<Json<Cv>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let (opp, row) = open_draft(&state, account, id, n, now).await?;
    sqlx::query(
        "UPDATE pipeline.cv_drafts
            SET status = 'discarded', updated_at = $2, preview_pdf = NULL
          WHERE id = $1 AND status = 'draft'",
    )
    .bind(row.id)
    .bind(now)
    .execute(&state.db)
    .await?;
    Ok(Json(build(&deps, &state.db, account, &opp, now).await?))
}
