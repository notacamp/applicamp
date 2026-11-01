//! Grill me's proposals: the facts taken from an answer about a job, waiting
//! for the person to keep, fix or drop them.
//!
//! The interview itself (a deep interview about one position at a time, one
//! model call per question) was retired from the client in #44 and removed
//! in #49. What stays is how a fact becomes part of a position: a tailored
//! CV's question is answered in [`about`], which stores the facts here as
//! proposals; keeping one writes it on the position in the profile document
//! (`work[].facts`), so the draft source check and every later CV treat it as
//! evidence. Nothing reaches the profile without a click.

pub mod about;
pub mod judge;
pub mod prompt;
#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row as _};
use utoipa::ToSchema;
use uuid::Uuid;

use self::judge::{Coverage, CoverageLevel, NewFact};
use crate::{
    features::{
        completion::{Current, check::Evidence, current, write_profile},
        imports::{DRAFT_LIFETIME, ExpireImports, source_check::squash},
    },
    schema::{Area, Profile, ProfileFact, Work},
};

/// Longest answer, in characters.
pub const MAX_ANSWER: usize = 2_000;

// ---- views ----------------------------------------------------------------

/// What a turn is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrillTurnKind {
    /// The interviewer asks.
    Question,
    /// The person answers.
    Answer,
    /// A short marker: `skipped` or `done`.
    Note,
}

/// One turn of the interview.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillTurn {
    /// Its place in the interview, from 1.
    pub seq: i32,
    /// What it is.
    pub kind: GrillTurnKind,
    /// The question, the answer, or `skipped` / `done` for a note.
    pub text: String,
    /// A question that pushes back on a vague answer or asks about a
    /// contradiction.
    pub push: bool,
    /// What it is about.
    pub area: Option<Area>,
    /// On an answer: lines the checks left out (a quote that was not in the
    /// answer, a number nobody wrote).
    pub dropped: i32,
}

/// Where an interview is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrillStatus {
    /// Nothing asked yet.
    NotStarted,
    /// Some areas are covered.
    InProgress,
    /// All seven areas are covered.
    Covered,
}

/// How well one area is covered.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct GrillAreaCoverage {
    /// The area.
    pub area: Area,
    /// How well.
    pub level: CoverageLevel,
}

/// The question waiting for an answer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillQuestion {
    /// The question.
    pub text: String,
    /// What it is about.
    pub area: Option<Area>,
    /// It pushes back, or asks about a contradiction.
    pub push: bool,
}

/// Where a fact taken from an answer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactProposalStatus {
    /// Waiting for Keep, Fix or Drop.
    Ready,
    /// Kept: it is on the position in the profile.
    Kept,
    /// Dropped.
    Dropped,
}

impl FactProposalStatus {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ready" => Some(Self::Ready),
            "kept" => Some(Self::Kept),
            "dropped" => Some(Self::Dropped),
            _ => None,
        }
    }
}

/// A fact taken from an answer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FactProposal {
    /// Its id.
    pub id: Uuid,
    /// The fact (as the person fixed it, once kept).
    pub text: String,
    /// The person's exact words it was taken from.
    pub quote: String,
    /// What it is about.
    pub area: Area,
    /// The answer it was taken from.
    pub turn_seq: i32,
    /// Waiting, kept or dropped.
    pub status: FactProposalStatus,
    /// When it is deleted if nobody keeps it.
    pub expires_at: DateTime<Utc>,
}

/// The position an interview is about.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillRole {
    /// Its title.
    pub position: Option<String>,
    /// Its company.
    pub company: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    pub start_date: Option<String>,
    /// As `start_date`; none while it is their current position.
    pub end_date: Option<String>,
}

fn role_summary(work: &Work) -> GrillRole {
    GrillRole {
        position: work.position.clone(),
        company: work.name.clone(),
        start_date: work.start_date.clone(),
        end_date: work.end_date.clone(),
    }
}

/// The facts waiting on one position, and how many are kept. The interview
/// fields (`status`, `coverage`, `turns`, `question`, `calls_today`,
/// `daily_cap`) are what is left of Grill me's interview (removed in #49):
/// always empty or zero, kept so the client's types stay as they are.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillView {
    /// The position's place in the profile's `work` list.
    pub position: u32,
    /// The position.
    pub role: GrillRole,
    /// Where the interview is.
    pub status: GrillStatus,
    /// The seven areas, in order.
    pub coverage: Vec<GrillAreaCoverage>,
    /// Areas at `ok`, of 7.
    pub covered: u8,
    /// The conversation so far.
    pub turns: Vec<GrillTurn>,
    /// The question waiting for an answer; none when there is none (before
    /// the start, after "done", or when everything is covered).
    pub question: Option<GrillQuestion>,
    /// Facts taken from answers in the last 24 hours, whether they wait for
    /// Keep, Fix or Drop, were kept or were dropped.
    pub proposals: Vec<FactProposal>,
    /// Facts the person has kept on this position, in all.
    pub kept: u32,
    /// Model calls made today by this account, of `daily_cap`.
    pub calls_today: u32,
    /// The most model calls an account can make for Grill me in a UTC day.
    pub daily_cap: u32,
}

/// What the person keeps.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct KeepFactRequest {
    /// The fact, fixed. Left out, the fact is kept as it is. Whatever is
    /// written must still be said by the quote or the profile; the quote
    /// itself never changes.
    pub text: Option<String>,
}

// ---- storage --------------------------------------------------------------

/// The position `work` as an interview is tied to it: renamed or moved
/// positions never lend their interview to another.
pub(crate) fn role_key(work: &Work) -> String {
    squash(&format!(
        "{}|{}|{}",
        work.name.as_deref().unwrap_or_default(),
        work.position.as_deref().unwrap_or_default(),
        work.start_date.as_deref().unwrap_or_default(),
    ))
}

struct Session {
    id: Uuid,
    role_key: String,
}

async fn session_of(db: &PgPool, account: Uuid, position: i32) -> Result<Option<Session>> {
    let row = sqlx::query(
        "SELECT id, role_key FROM profile.grill_sessions
          WHERE account_id = $1 AND position = $2",
    )
    .bind(account)
    .bind(position)
    .fetch_optional(db)
    .await?;
    row.map(|row| {
        Ok(Session {
            id: row.try_get("id")?,
            role_key: row.try_get("role_key")?,
        })
    })
    .transpose()
}

/// The session of `position` if it was started for this very position.
async fn session_for(
    db: &PgPool,
    account: Uuid,
    position: usize,
    work: &Work,
) -> Result<Option<Session>> {
    let index = i32::try_from(position).map_err(Error::internal)?;
    Ok(session_of(db, account, index)
        .await?
        .filter(|s| s.role_key == role_key(work)))
}

async fn proposals_of(db: &PgPool, session: Uuid, now: DateTime<Utc>) -> Result<Vec<FactProposal>> {
    let rows = sqlx::query(
        "SELECT id, text, quote, area, turn_seq, status, expires_at FROM profile.grill_proposals
          WHERE session_id = $1 AND expires_at > $2
          ORDER BY turn_seq, id",
    )
    .bind(session)
    .bind(now)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let area: String = row.try_get("area")?;
            let status: String = row.try_get("status")?;
            Ok(FactProposal {
                id: row.try_get("id")?,
                text: row.try_get("text")?,
                quote: row.try_get("quote")?,
                area: Area::parse(&area).ok_or_else(|| Error::internal("an unknown fact area"))?,
                turn_seq: row.try_get("turn_seq")?,
                status: FactProposalStatus::parse(&status)
                    .ok_or_else(|| Error::internal("an unknown fact status"))?,
                expires_at: row.try_get("expires_at")?,
            })
        })
        .collect()
}

fn coverage_view(coverage: &Coverage) -> Vec<GrillAreaCoverage> {
    Area::ALL
        .iter()
        .map(|area| GrillAreaCoverage {
            area: *area,
            level: coverage.level(*area),
        })
        .collect()
}

async fn view_of(
    state: &AppState,
    account: Uuid,
    current: &Current,
    position: usize,
) -> Result<GrillView> {
    let work = current.profile.work.get(position).ok_or(Error::NotFound)?;
    let session = session_for(&state.db, account, position, work).await?;
    let proposals = match &session {
        Some(s) => proposals_of(&state.db, s.id, state.clock.now()).await?,
        None => Vec::new(),
    };
    Ok(GrillView {
        position: u32::try_from(position).map_err(Error::internal)?,
        role: role_summary(work),
        status: GrillStatus::NotStarted,
        coverage: coverage_view(&Coverage::default()),
        covered: 0,
        turns: Vec::new(),
        question: None,
        proposals,
        kept: u32::try_from(work.facts.len()).unwrap_or(u32::MAX),
        calls_today: 0,
        daily_cap: 0,
    })
}

/// The profile and the position `position` of it; 404 for neither.
async fn load(state: &AppState, account: Uuid, position: u32) -> Result<(Current, usize)> {
    let current = current(&state.db, account).await?.ok_or(Error::NotFound)?;
    let index = usize::try_from(position).map_err(|_| Error::NotFound)?;
    if index >= current.profile.work.len() {
        return Err(Error::NotFound);
    }
    Ok((current, index))
}

/// Facts kept, waiting or dropped, squashed: not proposed again.
fn known_facts(work: &Work, waiting: &[FactProposal]) -> Vec<String> {
    work.facts
        .iter()
        .map(|f| squash(&f.text))
        .chain(waiting.iter().map(|p| squash(&p.text)))
        .collect()
}

async fn store_proposals(
    tx: &mut PgConnection,
    account: Uuid,
    session: Uuid,
    position: i32,
    turn_seq: i32,
    facts: &[NewFact],
    now: DateTime<Utc>,
) -> Result<Vec<FactProposal>> {
    let expires_at = now + DRAFT_LIFETIME;
    let mut stored = Vec::with_capacity(facts.len());
    for fact in facts {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO profile.grill_proposals
                (id, session_id, account_id, position, turn_seq, text, quote, area, status,
                 created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'ready', $9, $10)",
        )
        .bind(id)
        .bind(session)
        .bind(account)
        .bind(position)
        .bind(turn_seq)
        .bind(&fact.text)
        .bind(&fact.quote)
        .bind(fact.area.as_str())
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        stored.push(FactProposal {
            id,
            text: fact.text.clone(),
            quote: fact.quote.clone(),
            area: fact.area,
            turn_seq,
            status: FactProposalStatus::Ready,
            expires_at,
        });
    }
    if !facts.is_empty() {
        jobs::enqueue_at(&mut *tx, &ExpireImports {}, expires_at).await?;
    }
    Ok(stored)
}

fn position_index(position: usize) -> Result<i32> {
    i32::try_from(position).map_err(Error::internal)
}

/// The facts waiting on one position, and how many are kept. Never calls the
/// model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/grill/{position}",
    tag = "profile",
    operation_id = "profile.grill_role",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list.")),
    responses(
        (status = 200, description = "The interview.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(position): Path<u32>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let (current, index) = load(&state, account, position).await?;
    Ok(Json(view_of(&state, account, &current, index).await?))
}

struct ProposalRow {
    id: Uuid,
    position: i32,
    text: String,
    quote: String,
    area: Area,
    status: String,
    role_key: String,
}

async fn proposal_rows(
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    id: Option<Uuid>,
    position: Option<i32>,
) -> Result<Vec<ProposalRow>> {
    let rows = sqlx::query(
        "SELECT p.id, p.position, p.text, p.quote, p.area, p.status, s.role_key
           FROM profile.grill_proposals p
           JOIN profile.grill_sessions s ON s.id = p.session_id
          WHERE p.account_id = $1 AND p.expires_at > $2
            AND ($3::uuid IS NULL OR p.id = $3)
            AND ($4::int IS NULL OR p.position = $4)
          ORDER BY p.turn_seq, p.id",
    )
    .bind(account)
    .bind(now)
    .bind(id)
    .bind(position)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let area: String = row.try_get("area")?;
            Ok(ProposalRow {
                id: row.try_get("id")?,
                position: row.try_get("position")?,
                text: row.try_get("text")?,
                quote: row.try_get("quote")?,
                area: Area::parse(&area).ok_or_else(|| Error::internal("an unknown fact area"))?,
                status: row.try_get("status")?,
                role_key: row.try_get("role_key")?,
            })
        })
        .collect()
}

fn not_supported(claims: &[String]) -> Error {
    Error::BadRequest(
        "fact_not_supported",
        format!(
            "Your words and your profile do not say {}. Change the line, or drop those words.",
            claims.join(", ")
        ),
    )
}

/// Write `proposals` (all `ready`) into their positions' `facts` as one new
/// profile version. `fixed` is the person's edited text for the single
/// proposal; it is checked against its quote and the profile.
async fn keep_proposals(
    state: &AppState,
    account: Uuid,
    proposals: &[ProposalRow],
    fixed: Option<&str>,
) -> Result<()> {
    let mut tx = state.db.begin().await?;
    let row: Option<(sqlx::types::Json<Profile>,)> =
        sqlx::query_as("SELECT data FROM profile.profiles WHERE account_id = $1 FOR UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    let (profile,) = row.ok_or(Error::NotFound)?;
    let mut profile = profile.0;
    let evidence_source = serde_json::to_value(&profile).map_err(Error::internal)?;
    let now = state.clock.now();
    for proposal in proposals {
        let stale = || {
            Error::Conflict(
                "profile_changed",
                "That position changed since the fact was taken. Answer the question again."
                    .to_owned(),
            )
        };
        let work = usize::try_from(proposal.position)
            .ok()
            .and_then(|i| profile.work.get_mut(i))
            .filter(|work| role_key(work) == proposal.role_key)
            .ok_or_else(stale)?;
        let text = match fixed {
            Some(edited) => {
                let edited = edited.trim();
                if edited.is_empty() || edited.chars().count() > judge::MAX_FACT {
                    return Err(Error::BadRequest(
                        "invalid_fact",
                        format!("Write between 1 and {} characters.", judge::MAX_FACT),
                    ));
                }
                let unsaid = Evidence::of(&proposal.quote, &evidence_source).unsupported(edited);
                if !unsaid.is_empty() {
                    return Err(not_supported(&unsaid));
                }
                edited.to_owned()
            }
            None => proposal.text.clone(),
        };
        if !work.facts.iter().any(|f| squash(&f.text) == squash(&text)) {
            work.facts.push(ProfileFact {
                id: proposal.id.to_string(),
                text,
                quote: proposal.quote.clone(),
                area: proposal.area,
                kept_at: now,
                status: Default::default(),
                basis: None,
            });
        }
    }
    let profile = profile.tidied();
    profile.validate()?;
    let ids: Vec<Uuid> = proposals.iter().map(|p| p.id).collect();
    sqlx::query(
        "UPDATE profile.grill_proposals SET status = 'kept', text = coalesce($2, text)
          WHERE id = ANY($1)",
    )
    .bind(&ids)
    .bind(fixed.map(str::trim))
    .execute(&mut *tx)
    .await?;
    write_profile(state, &mut tx, account, &profile, Some("answer")).await?;
    tx.commit().await?;
    tracing::info!(facts = proposals.len(), "grill facts kept");
    Ok(())
}

/// Keep a fact, as it is or fixed.
///
/// Writes it on the position in the profile as a new profile version. A
/// fixed `text` is checked like the model's: it may say only what the quote
/// or the profile says (400 `fact_not_supported` names the words that are
/// not there); the quote never changes.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/proposals/{id}/keep",
    tag = "profile",
    operation_id = "profile.grill_keep",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The fact waiting.")),
    request_body = KeepFactRequest,
    responses(
        (status = 200, description = "The interview, after the fact was kept.", body = GrillView),
        (status = 400, description = "`invalid_fact`; `fact_not_supported`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such fact, or it expired", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_not_ready`: already kept or dropped; \
            `profile_changed`", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn keep_proposal(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<KeepFactRequest>, JsonRejection>,
) -> Result<Json<GrillView>> {
    let Json(keep) =
        body.map_err(|rejection| Error::BadRequest("invalid_fact", rejection.body_text()))?;
    let account = principal.account_id;
    let now = state.clock.now();
    let proposal = proposal_rows(&state.db, account, now, Some(id), None)
        .await?
        .into_iter()
        .next()
        .ok_or(Error::NotFound)?;
    if proposal.status != "ready" {
        return Err(Error::Conflict(
            "proposal_not_ready",
            "This fact was already kept or dropped.".to_owned(),
        ));
    }
    keep_proposals(
        &state,
        account,
        std::slice::from_ref(&proposal),
        keep.text.as_deref(),
    )
    .await?;
    let position = u32::try_from(proposal.position).map_err(Error::internal)?;
    let (current, index) = load(&state, account, position).await?;
    Ok(Json(view_of(&state, account, &current, index).await?))
}

/// Keep every fact waiting on a position, as they are. One profile version.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/{position}/keep-all",
    tag = "profile",
    operation_id = "profile.grill_keep_all",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list.")),
    responses(
        (status = 200, description = "The interview, after the facts were kept.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_changed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn keep_all(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(position): Path<u32>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let (_, index) = load(&state, account, position).await?;
    let now = state.clock.now();
    let waiting: Vec<ProposalRow> =
        proposal_rows(&state.db, account, now, None, Some(position_index(index)?))
            .await?
            .into_iter()
            .filter(|p| p.status == "ready")
            .collect();
    if !waiting.is_empty() {
        keep_proposals(&state, account, &waiting, None).await?;
    }
    let (current, index) = load(&state, account, position).await?;
    Ok(Json(view_of(&state, account, &current, index).await?))
}

/// Drop a fact: it is not kept and not shown again.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/proposals/{id}/drop",
    tag = "profile",
    operation_id = "profile.grill_drop",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The fact waiting.")),
    responses(
        (status = 200, description = "The interview, after the fact was dropped.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such fact, or it expired", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_not_ready`: already kept", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn drop_proposal(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let proposal = proposal_rows(&state.db, account, now, Some(id), None)
        .await?
        .into_iter()
        .next()
        .ok_or(Error::NotFound)?;
    match proposal.status.as_str() {
        "ready" => {
            sqlx::query("UPDATE profile.grill_proposals SET status = 'dropped' WHERE id = $1")
                .bind(id)
                .execute(&state.db)
                .await?;
        }
        "dropped" => {}
        _ => {
            return Err(Error::Conflict(
                "proposal_not_ready",
                "This fact was already kept.".to_owned(),
            ));
        }
    }
    let position = u32::try_from(proposal.position).map_err(Error::internal)?;
    let (current, index) = load(&state, account, position).await?;
    Ok(Json(view_of(&state, account, &current, index).await?))
}
