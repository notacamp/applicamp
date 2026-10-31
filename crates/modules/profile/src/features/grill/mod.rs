//! Grill me: a deep interview about one position at a time.
//!
//! The model asks one question, pushes back on vague answers, asks for
//! numbers and how the person knows them, and turns each answer into short
//! **facts** with the person's exact words. A fact waits as a *proposal* until
//! the person keeps it (as it is, or fixed), then it lives in the profile
//! document on the position (`work[].facts`), so the draft source check and
//! every later CV treat it as evidence. Nothing is saved without a click.
//!
//! One model call per start, answer or skip, no background calls, a hard cap
//! of [`DAILY_CAP`] calls per account per UTC day on top of the AI budget.
//! What the model says is trusted only as far as [`judge`] allows.

pub mod about;
pub mod judge;
pub mod prompt;
#[cfg(test)]
mod tests;

use applicamp_ai::{Caller, ChatRequest, Gateway, Message, ResponseFormat};
use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row as _};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use self::{
    judge::{Coverage, CoverageLevel, Input, NewFact, Outcome, Raw},
    prompt::{Context, Event, INSTRUCTIONS},
};
use crate::{
    features::{
        completion::{Current, carry_forward, check::Evidence, current, write_profile},
        imports::{Ai, DRAFT_LIFETIME, ExpireImports, source_check::squash},
    },
    schema::{Area, Profile, ProfileFact, Work},
};

/// Hard cap on model calls per account per UTC day.
pub const DAILY_CAP: i64 = 150;
/// Longest answer, in characters.
pub const MAX_ANSWER: usize = 2_000;
/// Answer budgets in tokens: the first attempt, and the one retry after a
/// cut-off (a reasoning model thinks first, and thinking counts).
const BUDGETS: [u32; 2] = [4_000, 8_000];
/// Most turns a view carries.
const MAX_TURNS_SHOWN: i64 = 400;

/// Whether `calls` already made today leave no room for another.
pub const fn over_cap(calls: i64) -> bool {
    calls >= DAILY_CAP
}

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

impl GrillTurnKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Note => "note",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "question" => Some(Self::Question),
            "answer" => Some(Self::Answer),
            "note" => Some(Self::Note),
            _ => None,
        }
    }
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

fn status_of(started: bool, covered: u8) -> GrillStatus {
    if usize::from(covered) == Area::ALL.len() {
        GrillStatus::Covered
    } else if started {
        GrillStatus::InProgress
    } else {
        GrillStatus::NotStarted
    }
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

/// The interview of one position.
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

/// One position in the list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillRoleItem {
    /// The position's place in the profile's `work` list.
    pub position: u32,
    /// The position.
    pub role: GrillRole,
    /// Where its interview is.
    pub status: GrillStatus,
    /// Areas at `ok`, of 7.
    pub covered: u8,
    /// Facts kept on it.
    pub kept: u32,
}

/// Every position with how far its interview is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GrillList {
    /// The positions, in the profile's order.
    pub roles: Vec<GrillRoleItem>,
    /// Model calls made today by this account, of `daily_cap`.
    pub calls_today: u32,
    /// The most model calls an account can make for Grill me in a UTC day.
    pub daily_cap: u32,
}

/// The language of the questions, as the client's.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LanguageQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
}

/// The person's answer.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct GrillAnswerRequest {
    /// In their own words, 1 to 2000 characters.
    pub text: String,
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
fn role_key(work: &Work) -> String {
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
    coverage: Coverage,
    skipped: Vec<Area>,
}

async fn session_of(db: &PgPool, account: Uuid, position: i32) -> Result<Option<Session>> {
    let row = sqlx::query(
        "SELECT id, role_key, coverage, skipped FROM profile.grill_sessions
          WHERE account_id = $1 AND position = $2",
    )
    .bind(account)
    .bind(position)
    .fetch_optional(db)
    .await?;
    row.map(|row| {
        let coverage: sqlx::types::Json<Coverage> = row.try_get("coverage")?;
        let skipped: sqlx::types::Json<Vec<Area>> = row.try_get("skipped")?;
        Ok(Session {
            id: row.try_get("id")?,
            role_key: row.try_get("role_key")?,
            coverage: coverage.0,
            skipped: skipped.0,
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

async fn turns_of(db: &PgPool, session: Uuid) -> Result<Vec<GrillTurn>> {
    // The last turns, in order.
    let rows = sqlx::query(
        "SELECT seq, kind, text, push, area, dropped FROM (
            SELECT seq, kind, text, push, area, dropped FROM profile.grill_turns
             WHERE session_id = $1 ORDER BY seq DESC LIMIT $2
         ) latest ORDER BY seq",
    )
    .bind(session)
    .bind(MAX_TURNS_SHOWN)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let kind: String = row.try_get("kind")?;
            let area: Option<String> = row.try_get("area")?;
            Ok(GrillTurn {
                seq: row.try_get("seq")?,
                kind: GrillTurnKind::parse(&kind)
                    .ok_or_else(|| Error::internal("an unknown turn kind"))?,
                text: row.try_get("text")?,
                push: row.try_get("push")?,
                area: area.as_deref().and_then(Area::parse),
                dropped: row.try_get("dropped")?,
            })
        })
        .collect()
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

/// Model calls `account` made since the start of the UTC day.
async fn calls_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM profile.grill_turns
          WHERE account_id = $1 AND ai_call AND created_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

/// Refuse a call that would be over the daily cap.
async fn ensure_room(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    let calls = calls_today(db, account, now).await?;
    if over_cap(calls) {
        return Err(Error::TooManyRequests(
            "grill_daily_cap",
            format!("Grill me is limited to {DAILY_CAP} questions a day. It continues tomorrow."),
        ));
    }
    Ok(calls)
}

fn counted(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
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

/// The open question: the last turn, when it is a question.
fn open_question(turns: &[GrillTurn]) -> Option<GrillQuestion> {
    turns
        .last()
        .filter(|t| t.kind == GrillTurnKind::Question)
        .map(|t| GrillQuestion {
            text: t.text.clone(),
            area: t.area,
            push: t.push,
        })
}

async fn view_of(
    state: &AppState,
    account: Uuid,
    current: &Current,
    position: usize,
) -> Result<GrillView> {
    let work = current.profile.work.get(position).ok_or(Error::NotFound)?;
    let now = state.clock.now();
    let session = session_for(&state.db, account, position, work).await?;
    let (coverage, turns, proposals) = match &session {
        Some(s) => (
            s.coverage.clone(),
            turns_of(&state.db, s.id).await?,
            proposals_of(&state.db, s.id, now).await?,
        ),
        None => (Coverage::default(), Vec::new(), Vec::new()),
    };
    let covered = coverage.ok_count();
    Ok(GrillView {
        position: u32::try_from(position).map_err(Error::internal)?,
        role: role_summary(work),
        status: status_of(!turns.is_empty(), covered),
        coverage: coverage_view(&coverage),
        covered,
        question: open_question(&turns),
        turns,
        proposals,
        kept: counted(i64::try_from(work.facts.len()).unwrap_or(i64::MAX)),
        calls_today: counted(calls_today(&state.db, account, now).await?),
        daily_cap: counted(DAILY_CAP),
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

fn gateway(ai: &Ai) -> Result<&Gateway> {
    match (&ai.0, ai.can_structure()) {
        (Some(gateway), true) => Ok(gateway),
        _ => Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    }
}

/// Ask the interviewer: one call, a second only after a cut-off. A spending
/// limit ends the attempt at once; nothing retries.
async fn ask(
    state: &AppState,
    gateway: &Gateway,
    account: Uuid,
    context: &Context<'_>,
) -> Result<Raw> {
    let caller = Caller {
        db: &state.db,
        account,
        now: state.clock.now(),
    };
    let mut last = Error::Unavailable(applicamp_ai::AI_FAILED);
    for max_tokens in BUDGETS {
        let request = ChatRequest {
            temperature: Some(0.0),
            max_tokens: Some(max_tokens),
            response_format: Some(ResponseFormat::json_schema(
                "profile_grill",
                prompt::schema(),
            )),
            ..ChatRequest::new(vec![
                Message::system(INSTRUCTIONS),
                Message::user(prompt::user_message(context)),
            ])
        };
        let reply = gateway.complete(caller, request).await?;
        if reply.finish_reason.as_deref() == Some("length") {
            tracing::warn!(max_tokens, "a grill answer was cut off");
            continue;
        }
        match reply.json::<Raw>() {
            Ok(raw) => return Ok(raw),
            Err(error) => {
                tracing::warn!(%error, "a grill answer was unusable");
                last = Error::from(error);
            }
        }
    }
    Err(last)
}

/// The facts still waiting for the person, as the model is shown them.
fn ready_texts(proposals: &[FactProposal]) -> Vec<(Area, String)> {
    proposals
        .iter()
        .filter(|p| p.status == FactProposalStatus::Ready)
        .map(|p| (p.area, p.text.clone()))
        .collect()
}

/// Facts kept, waiting or dropped, squashed: not proposed again.
fn known_facts(work: &Work, waiting: &[FactProposal]) -> Vec<String> {
    work.facts
        .iter()
        .map(|f| squash(&f.text))
        .chain(waiting.iter().map(|p| squash(&p.text)))
        .collect()
}

/// What `turns` last said: its last sequence number.
fn last_seq(turns: &[GrillTurn]) -> i32 {
    turns.last().map_or(0, |t| t.seq)
}

#[allow(clippy::too_many_arguments)]
async fn insert_turn(
    tx: &mut PgConnection,
    account: Uuid,
    session: Uuid,
    seq: i32,
    kind: GrillTurnKind,
    text: &str,
    push: bool,
    area: Option<Area>,
    dropped: u32,
    ai_call: bool,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO profile.grill_turns
            (session_id, seq, account_id, kind, text, push, area, dropped, ai_call, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(session)
    .bind(seq)
    .bind(account)
    .bind(kind.as_str())
    .bind(text)
    .bind(push)
    .bind(area.map(Area::as_str))
    .bind(i32::try_from(dropped).unwrap_or(i32::MAX))
    .bind(ai_call)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Lock the session and say where its turns end now.
async fn lock_and_last_seq(tx: &mut PgConnection, session: Uuid) -> Result<i32> {
    sqlx::query("SELECT 1 FROM profile.grill_sessions WHERE id = $1 FOR UPDATE")
        .bind(session)
        .execute(&mut *tx)
        .await?;
    Ok(sqlx::query_scalar(
        "SELECT coalesce(max(seq), 0) FROM profile.grill_turns WHERE session_id = $1",
    )
    .bind(session)
    .fetch_one(&mut *tx)
    .await?)
}

fn changed() -> Error {
    Error::Conflict(
        "grill_changed",
        "This interview changed while the question was being asked. Look again.".to_owned(),
    )
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

// ---- endpoints ------------------------------------------------------------

/// Every position with how far its interview is.
///
/// Computed from the stored interviews; never calls the model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/grill",
    tag = "profile",
    operation_id = "profile.grill_roles",
    security(("session" = [])),
    responses(
        (status = 200, description = "The positions.", body = GrillList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<GrillList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let Some(current) = current(&state.db, account).await? else {
        return Ok(Json(GrillList {
            roles: Vec::new(),
            calls_today: counted(calls_today(&state.db, account, now).await?),
            daily_cap: counted(DAILY_CAP),
        }));
    };
    let rows = sqlx::query(
        "SELECT s.position, s.role_key, s.coverage,
                (SELECT count(*) FROM profile.grill_turns t WHERE t.session_id = s.id) AS turns
           FROM profile.grill_sessions s WHERE s.account_id = $1",
    )
    .bind(account)
    .fetch_all(&state.db)
    .await?;
    let mut sessions: std::collections::HashMap<i32, (String, Coverage, i64)> = rows
        .into_iter()
        .map(|row| {
            let coverage: sqlx::types::Json<Coverage> = row.try_get("coverage")?;
            Ok((
                row.try_get("position")?,
                (row.try_get("role_key")?, coverage.0, row.try_get("turns")?),
            ))
        })
        .collect::<Result<_>>()?;
    let roles = current
        .profile
        .work
        .iter()
        .enumerate()
        .map(|(index, work)| {
            let session = i32::try_from(index)
                .ok()
                .and_then(|i| sessions.remove(&i))
                .filter(|(key, ..)| *key == role_key(work));
            let (covered, started) = session.map_or((0, false), |(_, coverage, turns)| {
                (coverage.ok_count(), turns > 0)
            });
            Ok(GrillRoleItem {
                position: u32::try_from(index).map_err(Error::internal)?,
                role: role_summary(work),
                status: status_of(started, covered),
                covered,
                kept: counted(i64::try_from(work.facts.len()).unwrap_or(i64::MAX)),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(GrillList {
        roles,
        calls_today: counted(calls_today(&state.db, account, now).await?),
        daily_cap: counted(DAILY_CAP),
    }))
}

/// The interview of one position: its turns, coverage, the facts waiting and
/// the question waiting for an answer. Never calls the model.
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

/// Start (or take up) the interview of a position.
///
/// With a question waiting, that one is shown again and the model is not
/// called. Otherwise this is **one model call** that makes the next question.
/// Areas skipped before are asked about again.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/{position}/start",
    tag = "profile",
    operation_id = "profile.grill_start",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list."),
        LanguageQuery),
    responses(
        (status = 200, description = "The interview, with its question.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`grill_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Path(position): Path<u32>,
    Query(query): Query<LanguageQuery>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let (current, index) = load(&state, account, position).await?;
    let work = &current.profile.work[index];
    let db_position = position_index(index)?;

    // A session made for another position at this place goes.
    if let Some(stale) = session_of(&state.db, account, db_position)
        .await?
        .filter(|s| s.role_key != role_key(work))
    {
        sqlx::query("DELETE FROM profile.grill_sessions WHERE id = $1")
            .bind(stale.id)
            .execute(&state.db)
            .await?;
    }
    let session = session_for(&state.db, account, index, work).await?;
    let turns = match &session {
        Some(s) => turns_of(&state.db, s.id).await?,
        None => Vec::new(),
    };
    let covered = session.as_ref().map_or(0, |s| s.coverage.ok_count());
    if open_question(&turns).is_some() || usize::from(covered) == Area::ALL.len() {
        return Ok(Json(view_of(&state, account, &current, index).await?));
    }

    let gateway = gateway(&ai)?;
    let now = state.clock.now();
    ensure_room(&state.db, account, now).await?;
    let coverage = session
        .as_ref()
        .map(|s| s.coverage.clone())
        .unwrap_or_default();
    let waiting = match &session {
        Some(s) => proposals_of(&state.db, s.id, now).await?,
        None => Vec::new(),
    };
    let waiting_texts = ready_texts(&waiting);
    let language = prompt::language(query.locale.as_deref());
    let raw = ask(
        &state,
        gateway,
        account,
        &Context {
            event: Event::Start,
            work,
            waiting: &waiting_texts,
            coverage: &coverage,
            skipped: &[],
            turns: &turns,
            answer: None,
            language: language.as_deref(),
        },
    )
    .await?;
    let profile_value = serde_json::to_value(&current.profile).map_err(Error::internal)?;
    let known = known_facts(work, &waiting);
    let outcome = judge::judge(
        raw,
        &Input {
            answer: None,
            profile: &profile_value,
            coverage: &coverage,
            skipped: &[],
            asked: None,
            known: &known,
        },
    );
    let Some(next) = outcome.next else {
        return Err(Error::Unavailable(applicamp_ai::AI_FAILED));
    };

    let mut tx = state.db.begin().await?;
    let session_id = match &session {
        Some(s) => s.id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO profile.grill_sessions
                    (id, account_id, position, role_key, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $5)
                 ON CONFLICT (account_id, position) DO UPDATE SET updated_at = excluded.updated_at
                 RETURNING id",
            )
            .bind(Uuid::now_v7())
            .bind(account)
            .bind(db_position)
            .bind(role_key(work))
            .bind(now)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let seq = lock_and_last_seq(&mut tx, session_id).await?;
    let seen = last_seq(&turns);
    if seq != seen {
        // Someone started it meanwhile: theirs stands.
        tx.rollback().await?;
        let current = self::current(&state.db, account)
            .await?
            .ok_or(Error::NotFound)?;
        return Ok(Json(view_of(&state, account, &current, index).await?));
    }
    sqlx::query("UPDATE profile.grill_sessions SET skipped = '[]', updated_at = $2 WHERE id = $1")
        .bind(session_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    insert_turn(
        &mut tx,
        account,
        session_id,
        seq + 1,
        GrillTurnKind::Question,
        &next.question,
        next.push,
        Some(next.area),
        0,
        true,
        now,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(prompt = prompt::PROMPT_VERSION, "a grill interview started");
    Ok(Json(view_of(&state, account, &current, index).await?))
}

/// Answer the question waiting.
///
/// **One model call** reads the answer: it returns the facts the answer
/// states (each with the person's exact words), what it covered and the next
/// question, which pushes back when the answer was vague. A fact whose quote
/// is not in the answer, or that says a number, name or tool neither the
/// quote nor the profile says, is left out and counted in the turn's
/// `dropped`. Nothing reaches the profile until a fact is kept.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/{position}/answer",
    tag = "profile",
    operation_id = "profile.grill_answer",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list."),
        LanguageQuery),
    request_body = GrillAnswerRequest,
    responses(
        (status = 200, description = "The interview, after the answer.", body = GrillView),
        (status = 400, description = "`invalid_answer`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position, or not started", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`no_open_question`; `grill_changed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`grill_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn answer(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Path(position): Path<u32>,
    Query(query): Query<LanguageQuery>,
    body: std::result::Result<Json<GrillAnswerRequest>, JsonRejection>,
) -> Result<Json<GrillView>> {
    let Json(GrillAnswerRequest { text }) =
        body.map_err(|rejection| Error::BadRequest("invalid_answer", rejection.body_text()))?;
    let text = text.trim().to_owned();
    if text.is_empty() || text.chars().count() > MAX_ANSWER {
        return Err(Error::BadRequest(
            "invalid_answer",
            format!("Write between 1 and {MAX_ANSWER} characters."),
        ));
    }
    let account = principal.account_id;
    let gateway = gateway(&ai)?;
    let (current, index) = load(&state, account, position).await?;
    let work = &current.profile.work[index];
    let session = session_for(&state.db, account, index, work)
        .await?
        .ok_or(Error::NotFound)?;
    let turns = turns_of(&state.db, session.id).await?;
    let asked = open_question(&turns).ok_or_else(|| {
        Error::Conflict(
            "no_open_question",
            "There is no question waiting. Start again.".to_owned(),
        )
    })?;
    let now = state.clock.now();
    ensure_room(&state.db, account, now).await?;
    let waiting = proposals_of(&state.db, session.id, now).await?;
    let waiting_texts = ready_texts(&waiting);
    let language = prompt::language(query.locale.as_deref());
    let raw = ask(
        &state,
        gateway,
        account,
        &Context {
            event: Event::Answer,
            work,
            waiting: &waiting_texts,
            coverage: &session.coverage,
            skipped: &session.skipped,
            turns: &turns,
            answer: Some(&text),
            language: language.as_deref(),
        },
    )
    .await?;
    let profile_value = serde_json::to_value(&current.profile).map_err(Error::internal)?;
    let known = known_facts(work, &waiting);
    let outcome: Outcome = judge::judge(
        raw,
        &Input {
            answer: Some(&text),
            profile: &profile_value,
            coverage: &session.coverage,
            skipped: &session.skipped,
            asked: asked.area,
            known: &known,
        },
    );

    let mut tx = state.db.begin().await?;
    let seq = lock_and_last_seq(&mut tx, session.id).await?;
    if seq != last_seq(&turns) {
        return Err(changed());
    }
    insert_turn(
        &mut tx,
        account,
        session.id,
        seq + 1,
        GrillTurnKind::Answer,
        &text,
        false,
        asked.area,
        outcome.dropped,
        true,
        now,
    )
    .await?;
    store_proposals(
        &mut tx,
        account,
        session.id,
        position_index(index)?,
        seq + 1,
        &outcome.facts,
        now,
    )
    .await?;
    sqlx::query("UPDATE profile.grill_sessions SET coverage = $2, updated_at = $3 WHERE id = $1")
        .bind(session.id)
        .bind(sqlx::types::Json(&outcome.coverage))
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if let Some(next) = &outcome.next {
        insert_turn(
            &mut tx,
            account,
            session.id,
            seq + 2,
            GrillTurnKind::Question,
            &next.question,
            next.push,
            Some(next.area),
            0,
            false,
            now,
        )
        .await?;
    }
    tx.commit().await?;
    // Ids, counts and kinds only: never the answer.
    tracing::info!(
        prompt = prompt::PROMPT_VERSION,
        facts = outcome.facts.len(),
        dropped = outcome.dropped,
        covered = outcome.coverage.ok_count(),
        "a grill answer read"
    );
    Ok(Json(view_of(&state, account, &current, index).await?))
}

/// Skip the question waiting.
///
/// Its area is left alone until the interview is started again, and the
/// next question is made: **one model call**, or none when nothing is left
/// to ask.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/{position}/skip",
    tag = "profile",
    operation_id = "profile.grill_skip",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list."),
        LanguageQuery),
    responses(
        (status = 200, description = "The interview, after the skip.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position, or not started", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`no_open_question`; `grill_changed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`grill_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn skip(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Path(position): Path<u32>,
    Query(query): Query<LanguageQuery>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let (current, index) = load(&state, account, position).await?;
    let work = &current.profile.work[index];
    let session = session_for(&state.db, account, index, work)
        .await?
        .ok_or(Error::NotFound)?;
    let turns = turns_of(&state.db, session.id).await?;
    let asked = open_question(&turns).ok_or_else(|| {
        Error::Conflict(
            "no_open_question",
            "There is no question waiting. Start again.".to_owned(),
        )
    })?;
    let now = state.clock.now();
    let mut skipped = session.skipped.clone();
    if let Some(area) = asked.area
        && !skipped.contains(&area)
    {
        skipped.push(area);
    }

    // Something left to ask: one call. Nothing left: none.
    let mut next = None;
    let mut called = false;
    if session.coverage.first_open(&skipped).is_some() {
        let gateway = gateway(&ai)?;
        ensure_room(&state.db, account, now).await?;
        let waiting = proposals_of(&state.db, session.id, now).await?;
        let waiting_texts = ready_texts(&waiting);
        let language = prompt::language(query.locale.as_deref());
        let raw = ask(
            &state,
            gateway,
            account,
            &Context {
                event: Event::Skip,
                work,
                waiting: &waiting_texts,
                coverage: &session.coverage,
                skipped: &skipped,
                turns: &turns,
                answer: None,
                language: language.as_deref(),
            },
        )
        .await?;
        let profile_value = serde_json::to_value(&current.profile).map_err(Error::internal)?;
        let known = known_facts(work, &waiting);
        next = judge::judge(
            raw,
            &Input {
                answer: None,
                profile: &profile_value,
                coverage: &session.coverage,
                skipped: &skipped,
                asked: asked.area,
                known: &known,
            },
        )
        .next;
        called = true;
    }

    let mut tx = state.db.begin().await?;
    let seq = lock_and_last_seq(&mut tx, session.id).await?;
    if seq != last_seq(&turns) {
        return Err(changed());
    }
    insert_turn(
        &mut tx,
        account,
        session.id,
        seq + 1,
        GrillTurnKind::Note,
        "skipped",
        false,
        asked.area,
        0,
        called,
        now,
    )
    .await?;
    sqlx::query("UPDATE profile.grill_sessions SET skipped = $2, updated_at = $3 WHERE id = $1")
        .bind(session.id)
        .bind(sqlx::types::Json(&skipped))
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if let Some(next) = &next {
        insert_turn(
            &mut tx,
            account,
            session.id,
            seq + 2,
            GrillTurnKind::Question,
            &next.question,
            next.push,
            Some(next.area),
            0,
            false,
            now,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(view_of(&state, account, &current, index).await?))
}

/// End the interview for now. No model call; the question waiting is put
/// away, and starting again makes a new one. Facts waiting stay for their
/// 24 hours.
#[utoipa::path(
    post,
    path = "/api/v1/profile/grill/{position}/done",
    tag = "profile",
    operation_id = "profile.grill_done",
    security(("session" = [])),
    params(("position" = u32, Path, description = "The position's place in the profile's `work` list.")),
    responses(
        (status = 200, description = "The interview.", body = GrillView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position, or not started", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn done(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(position): Path<u32>,
) -> Result<Json<GrillView>> {
    let account = principal.account_id;
    let (current, index) = load(&state, account, position).await?;
    let work = &current.profile.work[index];
    let session = session_for(&state.db, account, index, work)
        .await?
        .ok_or(Error::NotFound)?;
    let mut tx = state.db.begin().await?;
    let seq = lock_and_last_seq(&mut tx, session.id).await?;
    let last: Option<String> = sqlx::query_scalar(
        "SELECT kind FROM profile.grill_turns WHERE session_id = $1 AND seq = $2",
    )
    .bind(session.id)
    .bind(seq)
    .fetch_optional(&mut *tx)
    .await?;
    if last.as_deref() == Some("question") {
        insert_turn(
            &mut tx,
            account,
            session.id,
            seq + 1,
            GrillTurnKind::Note,
            "done",
            false,
            None,
            0,
            false,
            state.clock.now(),
        )
        .await?;
    }
    tx.commit().await?;
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
    let row: Option<(sqlx::types::Json<Profile>, i32)> = sqlx::query_as(
        "SELECT data, version FROM profile.profiles WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?;
    let (profile, version) = row.ok_or(Error::NotFound)?;
    let mut profile = profile.0;
    let evidence_source = serde_json::to_value(&profile).map_err(Error::internal)?;
    let now = state.clock.now();
    for proposal in proposals {
        let stale = || {
            Error::Conflict(
                "profile_changed",
                "That position changed since the fact was taken. Start the interview again."
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
    let new_version = write_profile(state, &mut tx, account, &profile, Some("answer")).await?;
    // The person's own kept fact does not make the gap review stale.
    carry_forward(&mut tx, account, version, new_version, None).await?;
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
