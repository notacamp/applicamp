//! Your LinkedIn (#84): a check-up of the LinkedIn profile the person
//! uploads, against their profile and goals, with what to change on LinkedIn.
//!
//! **The check-up** (`POST /api/v1/profile/linkedin`) reads the LinkedIn
//! profile from the file (the data export ZIP without a model, as the import
//! does; the profile saved as PDF through the import's readers, two model
//! calls), keeps it without the person's name or contact details
//! ([`prompt::scrubbed`]), then makes **one model call** for the review: an
//! overall read, one finding per part marked good, could be better or
//! missing with why, and proposals (a headline, the About text, a position's
//! description, skills to add), each citing the sources it rests on.
//!
//! **The judge** ([`judge`]) is plain code: a claim no cited source says, or
//! a buzzword, is a flag on the proposal, shown and never dropped. A flagged
//! proposal cannot be kept until it is edited to pass (checked again against
//! the same sources) or dropped.
//!
//! **Keep, Drop, Done** (`PATCH …/proposals/{id}`) are each reversible; a
//! kept proposal is on the list of things to change on LinkedIn, with its
//! text to copy. **Start over** (`DELETE`) throws the review away; a new
//! check-up replaces it. Nothing is written to the profile or the goals, and
//! nothing is fetched from LinkedIn or written there (ADR 0006).
//!
//! One review per account, 30 days after its last touch (`profile.expire_imports`
//! sweeps it). Check-ups are capped at [`DAILY_CAP`] a day; every call counts
//! against the AI budget.

pub mod judge;
pub mod prompt;
#[cfg(test)]
mod tests;

use applicamp_ai::{Caller, ChatRequest, Gateway, Message, ResponseFormat};
use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{Multipart, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use chrono::{DateTime, Datelike as _, TimeDelta, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use self::judge::{
    LinkedInFinding, LinkedInPart, LinkedInProposal, LinkedInState, ProposalStatus, RawReview,
};
use crate::{
    features::{
        completion::{Current, current},
        grill::prompt::language,
        imports::{
            self, Ai, ExpireImports, ImportKind, ImportUpload, MAX_PDF_BYTES, MAX_TEXT_CHARS,
        },
    },
    schema::Profile,
};

/// Most check-ups an account may make in a UTC day.
pub const DAILY_CAP: i64 = 10;
/// How long a review lives after its last touch.
pub const LIFETIME: TimeDelta = TimeDelta::days(30);
/// Answer budgets in tokens: the first attempt, and the one retry after a
/// cut-off (a reasoning model thinks first, and thinking counts).
const BUDGETS: [u32; 2] = [6_000, 10_000];

// ---- views ----------------------------------------------------------------

/// What a check-up is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedInKind {
    /// The LinkedIn data export ZIP, read without a model.
    LinkedinZip,
    /// The LinkedIn profile saved as PDF, read by the model.
    Pdf,
}

impl LinkedInKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LinkedinZip => "linkedin_zip",
            Self::Pdf => "pdf",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match ImportKind::parse(raw)? {
            ImportKind::LinkedinZip => Some(Self::LinkedinZip),
            ImportKind::Pdf => Some(Self::Pdf),
            ImportKind::Text => None,
        }
    }
}

/// The file a check-up is made from.
#[derive(Debug, Default, ToSchema)]
pub struct LinkedInUpload {
    /// `linkedin_zip` or `pdf`.
    #[schema(value_type = LinkedInKind)]
    pub kind: String,
    /// The file: the ZIP (up to 20 MB) or the PDF (up to 8 MB).
    #[schema(value_type = String, format = Binary)]
    pub file: String,
}

/// One finding, as shown: its part, what LinkedIn says there now, its state,
/// why, and its proposals' ids.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkedInFindingView {
    pub id: String,
    pub part: LinkedInPart,
    /// For a position: its index in the LinkedIn profile.
    pub position: Option<u32>,
    /// For a position: "Title at Company".
    pub title: Option<String>,
    /// What LinkedIn says in this part now; none when it is empty.
    pub now: Option<String>,
    pub state: LinkedInState,
    pub why: String,
    pub proposals: Vec<String>,
}

/// A review of the LinkedIn profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkedInReview {
    pub kind: LinkedInKind,
    /// The language it was written in.
    pub language: String,
    pub made_at: DateTime<Utc>,
    /// The profile changed since the review was made.
    pub stale: bool,
    /// How the LinkedIn profile reads to a recruiter for the wanted roles.
    pub overall: String,
    pub findings: Vec<LinkedInFindingView>,
    pub proposals: Vec<LinkedInProposal>,
    /// Proposals waiting for Keep or Drop.
    pub open: u32,
    /// Proposals kept, still to change on LinkedIn.
    pub kept: u32,
    /// Proposals changed on LinkedIn.
    pub done: u32,
}

/// Where the check-up is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkedInView {
    /// There is a profile to compare with.
    pub has_profile: bool,
    /// A model is set up, so a check-up can be made.
    pub ai_available: bool,
    /// The review, if one was made and not thrown away.
    pub review: Option<LinkedInReview>,
    /// Check-ups made today by this account, of `daily_cap`.
    pub checkups_today: u32,
    /// The most check-ups an account can make in a UTC day.
    pub daily_cap: u32,
}

// ---- storage --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Row {
    kind: String,
    language: String,
    profile_version: i32,
    linkedin: sqlx::types::Json<Profile>,
    overall: String,
    findings: sqlx::types::Json<Vec<LinkedInFinding>>,
    proposals: sqlx::types::Json<Vec<LinkedInProposal>>,
    created_at: DateTime<Utc>,
}

async fn row_of(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "SELECT kind, language, profile_version, linkedin, overall, findings, proposals, created_at
           FROM profile.linkedin_reviews WHERE account_id = $1 AND expires_at > $2",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(db)
    .await?)
}

async fn checkups_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM profile.linkedin_calls WHERE account_id = $1 AND created_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

fn counted(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn count_of(proposals: &[LinkedInProposal], status: ProposalStatus) -> u32 {
    counted(
        i64::try_from(proposals.iter().filter(|p| p.status == status).count()).unwrap_or(i64::MAX),
    )
}

/// What LinkedIn says in `part` now, from the profile as it was read.
fn now_of(linkedin: &Profile, part: LinkedInPart, position: Option<u32>) -> Option<String> {
    let joined = |items: Vec<String>| {
        let items: Vec<String> = items.into_iter().filter(|i| !i.trim().is_empty()).collect();
        (!items.is_empty()).then(|| items.join(", "))
    };
    let text = match part {
        LinkedInPart::Headline => linkedin.basics.label.clone(),
        LinkedInPart::Summary => linkedin.basics.summary.clone(),
        LinkedInPart::Position => {
            let work = linkedin.work.get(usize::try_from(position?).ok()?)?;
            let mut parts = work.summary.clone().into_iter().collect::<Vec<_>>();
            parts.extend(work.highlights.iter().cloned());
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        LinkedInPart::Skills => joined(linkedin.skills.iter().map(|s| s.name.clone()).collect()),
        LinkedInPart::Education => joined(
            linkedin
                .education
                .iter()
                .map(|e| {
                    let degree = [e.study_type.as_deref(), e.area.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if degree.is_empty() {
                        e.institution.clone()
                    } else {
                        format!("{degree}, {}", e.institution)
                    }
                })
                .collect(),
        ),
        LinkedInPart::Languages => joined(
            linkedin
                .languages
                .iter()
                .map(|l| match &l.fluency {
                    Some(fluency) => format!("{} ({fluency})", l.language),
                    None => l.language.clone(),
                })
                .collect(),
        ),
        LinkedInPart::Certifications => joined(
            linkedin
                .certificates
                .iter()
                .map(|c| c.name.clone())
                .collect(),
        ),
        LinkedInPart::Projects => {
            joined(linkedin.projects.iter().map(|p| p.name.clone()).collect())
        }
    };
    text.filter(|t| !t.trim().is_empty())
}

impl Row {
    fn into_review(self, profile_version: i32) -> Result<LinkedInReview> {
        let kind = LinkedInKind::parse(&self.kind)
            .ok_or_else(|| Error::internal("a LinkedIn review of an unknown kind"))?;
        let linkedin = self.linkedin.0;
        let findings = self
            .findings
            .0
            .into_iter()
            .map(|f| LinkedInFindingView {
                title: f.position.and_then(|p| {
                    linkedin
                        .work
                        .get(usize::try_from(p).ok()?)
                        .map(prompt::role_name)
                        .filter(|t| !t.is_empty())
                }),
                now: now_of(&linkedin, f.part, f.position),
                id: f.id,
                part: f.part,
                position: f.position,
                state: f.state,
                why: f.why,
                proposals: f.proposals,
            })
            .collect();
        let proposals = self.proposals.0;
        Ok(LinkedInReview {
            kind,
            language: self.language,
            made_at: self.created_at,
            stale: self.profile_version != profile_version,
            overall: self.overall,
            findings,
            open: count_of(&proposals, ProposalStatus::Ready),
            kept: count_of(&proposals, ProposalStatus::Kept),
            done: count_of(&proposals, ProposalStatus::Done),
            proposals,
        })
    }
}

async fn view(state: &AppState, ai: &Ai, account: Uuid) -> Result<LinkedInView> {
    let now = state.clock.now();
    let seen = current(&state.db, account).await?;
    let review = match row_of(&state.db, account, now).await? {
        Some(row) => Some(row.into_review(seen.as_ref().map_or(-1, |s| s.version))?),
        None => None,
    };
    Ok(LinkedInView {
        has_profile: seen.is_some(),
        ai_available: ai.can_structure(),
        review,
        checkups_today: counted(checkups_today(&state.db, account, now).await?),
        daily_cap: counted(DAILY_CAP),
    })
}

/// The model, or `ai_unavailable`.
fn model_of(ai: &Ai) -> Result<&Gateway> {
    match (&ai.0, ai.can_structure()) {
        (Some(gateway), true) => Ok(gateway),
        _ => Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    }
}

fn needs_profile() -> Error {
    Error::Conflict(
        "profile_needed",
        "Add your profile first (import it or write it), then check your LinkedIn.".to_owned(),
    )
}

fn over_cap() -> Error {
    Error::TooManyRequests(
        "linkedin_daily_cap",
        format!("This is limited to {DAILY_CAP} check-ups a day. It continues tomorrow."),
    )
}

/// Ask `system` / `user` for a JSON object shaped like `schema`: one call, a
/// second only after a cut-off. A spending limit ends it at once.
async fn ask<T: DeserializeOwned>(
    gateway: &Gateway,
    caller: Caller<'_>,
    schema: &Value,
    system: &str,
    user: &str,
) -> Result<T> {
    let mut last = Error::Unavailable(applicamp_ai::AI_FAILED);
    for max_tokens in BUDGETS {
        let request = ChatRequest {
            temperature: Some(0.0),
            max_tokens: Some(max_tokens),
            response_format: Some(ResponseFormat::json_schema(
                "linkedin_review",
                schema.clone(),
            )),
            ..ChatRequest::new(vec![Message::system(system), Message::user(user)])
        };
        let reply = gateway.complete(caller, request).await?;
        if reply.finish_reason.as_deref() == Some("length") {
            tracing::warn!(max_tokens, "a LinkedIn review was cut off");
            continue;
        }
        match reply.json::<T>() {
            Ok(read) => return Ok(read),
            Err(error) => {
                tracing::warn!(%error, "a LinkedIn review was unusable");
                last = Error::from(error);
            }
        }
    }
    Err(last)
}

// ---- the check-up -----------------------------------------------------------

/// The language of what the model writes.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LinkedInQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
}

/// The LinkedIn profile in the upload, as the import reads it: the ZIP's
/// CSVs (no model), or the PDF's text structured by the model.
async fn read_upload(
    kind: LinkedInKind,
    upload: ImportUpload,
    gateway: &Gateway,
    caller: Caller<'_>,
) -> Result<Profile> {
    let bytes = upload.file.ok_or_else(|| {
        Error::BadRequest(
            "invalid_upload",
            "Choose the ZIP LinkedIn sent you, or your profile saved as PDF.".to_owned(),
        )
    })?;
    let linkedin = match kind {
        LinkedInKind::LinkedinZip => {
            tokio::task::spawn_blocking(move || imports::linkedin::read_export(&bytes))
                .await
                .map_err(Error::internal)??
        }
        LinkedInKind::Pdf => {
            if bytes.len() > MAX_PDF_BYTES {
                return Err(Error::BadRequest(
                    "file_too_large",
                    "This PDF is over 8 MB.".to_owned(),
                ));
            }
            let text = imports::pdf::text_of(bytes).await?;
            if text.chars().count() > MAX_TEXT_CHARS {
                return Err(Error::BadRequest(
                    "text_too_long",
                    format!("This PDF holds more than {MAX_TEXT_CHARS} characters of text."),
                ));
            }
            imports::structure::structure(Some(gateway), caller, &text)
                .await?
                .0
        }
    };
    let (linkedin, _unfit) = linkedin.sanitized();
    if linkedin.is_empty() {
        return Err(Error::BadRequest(
            "nothing_found",
            "We found no profile in this. Check it is the right file.".to_owned(),
        ));
    }
    Ok(prompt::scrubbed(linkedin))
}

/// Make the review of `linkedin` for `seen` (one model call) and keep it as
/// the account's review.
#[allow(clippy::too_many_arguments)]
async fn review(
    state: &AppState,
    gateway: &Gateway,
    account: Uuid,
    seen: &Current,
    kind: LinkedInKind,
    linkedin: Profile,
    locale: Option<String>,
    now: DateTime<Utc>,
) -> Result<()> {
    let caller = Caller {
        db: &state.db,
        account,
        now,
    };
    let sources = prompt::all_sources(&seen.profile, &seen.goals, &linkedin, now.year());
    let message = prompt::message(
        &sources,
        &seen.goals,
        &linkedin,
        locale.as_deref(),
        &now.format("%Y-%m-%d").to_string(),
    );
    let raw: RawReview = ask(
        gateway,
        caller,
        &prompt::schema(),
        prompt::INSTRUCTIONS,
        &message,
    )
    .await?;
    let known: Vec<String> = linkedin.skills.iter().map(|s| s.name.clone()).collect();
    let judged = judge::judge(raw, &sources, linkedin.work.len(), &known);
    let flagged = judged
        .proposals
        .iter()
        .filter(|p| !p.flags.is_empty())
        .count();
    let expires_at = now + LIFETIME;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO profile.linkedin_reviews (account_id, kind, language, profile_version, \
         goals_version, linkedin, overall, findings, proposals, created_at, updated_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11) \
         ON CONFLICT (account_id) DO UPDATE SET kind = EXCLUDED.kind, language = EXCLUDED.language, \
         profile_version = EXCLUDED.profile_version, goals_version = EXCLUDED.goals_version, \
         linkedin = EXCLUDED.linkedin, overall = EXCLUDED.overall, findings = EXCLUDED.findings, \
         proposals = EXCLUDED.proposals, created_at = EXCLUDED.created_at, \
         updated_at = EXCLUDED.updated_at, expires_at = EXCLUDED.expires_at",
    )
    .bind(account)
    .bind(kind.as_str())
    .bind(locale.unwrap_or_else(|| "en".to_owned()))
    .bind(seen.version)
    .bind(seen.goals_version)
    .bind(sqlx::types::Json(&linkedin))
    .bind(&judged.overall)
    .bind(sqlx::types::Json(&judged.findings))
    .bind(sqlx::types::Json(&judged.proposals))
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireImports {}, expires_at).await?;
    tx.commit().await?;
    // Ids and counts only.
    tracing::info!(
        kind = kind.as_str(),
        prompt = prompt::PROMPT_VERSION,
        findings = judged.findings.len(),
        proposals = judged.proposals.len(),
        flagged,
        "LinkedIn review made"
    );
    Ok(())
}

/// Where the check-up is: the review, if any, and whether one can be made.
/// Never calls a model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/linkedin",
    tag = "profile",
    operation_id = "profile.linkedin",
    security(("session" = [])),
    responses(
        (status = 200, description = "The check-up's state.", body = LinkedInView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
) -> Result<Json<LinkedInView>> {
    Ok(Json(view(&state, &ai, principal.account_id).await?))
}

/// Check the LinkedIn profile in the file against the profile and goals: the
/// file is read as the import reads it (never stored), then one model call
/// writes the review, which replaces the account's. The LinkedIn profile is
/// kept without the person's name or contact details.
#[utoipa::path(
    post,
    path = "/api/v1/profile/linkedin",
    tag = "profile",
    operation_id = "profile.linkedin_check",
    security(("session" = [])),
    params(LinkedInQuery),
    request_body(content = LinkedInUpload, content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "The check-up's state, with the review.", body = LinkedInView),
        (status = 400, description = "`invalid_upload`, `invalid_kind`, `file_too_large`, \
            `text_too_long`, `not_a_zip`, `archive_too_many_entries`, \
            `archive_entry_too_large`, `archive_too_large`, `archive_unsafe_path`, \
            `not_a_pdf`, `pdf_unreadable`, `pdf_too_many_pages`, `pdf_no_text`, \
            `nothing_found`", body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`linkedin_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn check(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Query(query): Query<LinkedInQuery>,
    form: Multipart,
) -> Result<(StatusCode, Json<LinkedInView>)> {
    let account = principal.account_id;
    let upload = ImportUpload::read(form).await?;
    let kind = upload
        .kind
        .as_deref()
        .and_then(LinkedInKind::parse)
        .ok_or_else(|| {
            Error::BadRequest(
                "invalid_kind",
                "Say what you are uploading: linkedin_zip or pdf.".to_owned(),
            )
        })?;
    let gateway = model_of(&ai)?;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(needs_profile)?;
    let now = state.clock.now();
    if checkups_today(&state.db, account, now).await? >= DAILY_CAP {
        return Err(over_cap());
    }
    let caller = Caller {
        db: &state.db,
        account,
        now,
    };
    let linkedin = read_upload(kind, upload, gateway, caller).await?;
    // A check-up counts once the file is read, so a wrong file costs nothing.
    sqlx::query(
        "INSERT INTO profile.linkedin_calls (id, account_id, created_at) VALUES ($1, $2, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(now)
    .execute(&state.db)
    .await?;
    let locale = language(query.locale.as_deref());
    review(&state, gateway, account, &seen, kind, linkedin, locale, now).await?;
    Ok((StatusCode::CREATED, Json(view(&state, &ai, account).await?)))
}

// ---- the decisions ------------------------------------------------------------

/// What the person does with a proposal: a status, a rewrite, or both.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposalDecision {
    /// `kept`, `dropped`, `done`, or `ready` to take a decision back.
    #[serde(default)]
    pub status: Option<ProposalStatus>,
    /// The text as the person rewrote it; checked again against the same
    /// sources. A rewrite of a kept or done proposal makes it `ready` again
    /// unless a status is given too.
    #[serde(default)]
    pub text: Option<String>,
}

fn invalid_decision() -> Error {
    Error::BadRequest(
        "invalid_decision",
        "Say a status (ready, kept, dropped, done), a text, or both.".to_owned(),
    )
}

/// Keep, drop or mark done one proposal, or rewrite it. Keep is refused while
/// a flag remains; Done is allowed only on a kept proposal. Every decision can
/// be taken back with `ready`.
#[utoipa::path(
    patch,
    path = "/api/v1/profile/linkedin/proposals/{id}",
    tag = "profile",
    operation_id = "profile.linkedin_decide",
    security(("session" = [])),
    params(("id" = String, Path, description = "The proposal's id in the review.")),
    request_body = ProposalDecision,
    responses(
        (status = 200, description = "The check-up's state.", body = LinkedInView),
        (status = 400, description = "`invalid_decision`; `invalid_text`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no review, or no such proposal", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_flagged`: it cannot be kept as it is; \
            `proposal_not_kept`: only a kept proposal can be done", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn decide(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Path(id): Path<String>,
    body: std::result::Result<Json<ProposalDecision>, JsonRejection>,
) -> Result<Json<LinkedInView>> {
    let Json(decision) = body.map_err(|_| invalid_decision())?;
    if decision.status.is_none() && decision.text.is_none() {
        return Err(invalid_decision());
    }
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let proposals: Option<sqlx::types::Json<Vec<LinkedInProposal>>> = sqlx::query_scalar(
        "SELECT proposals FROM profile.linkedin_reviews
          WHERE account_id = $1 AND expires_at > $2 FOR UPDATE",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let mut proposals = proposals.ok_or(Error::NotFound)?.0;
    let proposal = proposals
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(Error::NotFound)?;
    if let Some(text) = decision.text {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > proposal.field.max() {
            return Err(Error::BadRequest(
                "invalid_text",
                format!("Write between 1 and {} characters.", proposal.field.max()),
            ));
        }
        proposal.text = text.to_owned();
        proposal.flags = judge::flags_of(text, &proposal.sources);
        proposal.edited = true;
        proposal.status = ProposalStatus::Ready;
    }
    if let Some(status) = decision.status {
        match status {
            ProposalStatus::Kept if !proposal.flags.is_empty() => {
                return Err(Error::Conflict(
                    "proposal_flagged",
                    "This says something your profile doesn't. Edit it to say only what your \
                     profile says, or drop it."
                        .to_owned(),
                ));
            }
            ProposalStatus::Done
                if !matches!(proposal.status, ProposalStatus::Kept | ProposalStatus::Done) =>
            {
                return Err(Error::Conflict(
                    "proposal_not_kept",
                    "Keep it first, then mark it done once it is on LinkedIn.".to_owned(),
                ));
            }
            _ => proposal.status = status,
        }
    }
    let expires_at = now + LIFETIME;
    sqlx::query(
        "UPDATE profile.linkedin_reviews SET proposals = $2, updated_at = $3, expires_at = $4
          WHERE account_id = $1",
    )
    .bind(account)
    .bind(sqlx::types::Json(&proposals))
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut tx, &ExpireImports {}, expires_at).await?;
    tx.commit().await?;
    Ok(Json(view(&state, &ai, account).await?))
}

/// Start over: throw the review away. Nothing else changes.
#[utoipa::path(
    delete,
    path = "/api/v1/profile/linkedin",
    tag = "profile",
    operation_id = "profile.linkedin_discard",
    security(("session" = [])),
    responses(
        (status = 204, description = "Gone (or there was none)."),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn discard(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<StatusCode> {
    sqlx::query("DELETE FROM profile.linkedin_reviews WHERE account_id = $1")
        .bind(principal.account_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
