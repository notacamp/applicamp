//! Tell me about you (#43): what Applicamp knows and assumes about a
//! person, confirmed by them.
//!
//! **The portrait** (`/api/v1/profile/about/portrait`) is one model call per
//! profile version: sentences marked by where each comes from. `known` ones
//! come from the profile, `told` ones from facts the person wrote, `assumed`
//! ones are Applicamp's guesses (what is usual for their roles, and what a
//! past employer's own website says), each with its reason. The person
//! settles each assumption: *That's right* (a confirmed fact, saved at Save),
//! *That's wrong* (struck out, not suggested again for this version) or *Ask
//! me later*. A `known` or `told` sentence the claim check cannot find in the
//! sources it cites is **downgraded to assumed, never dropped**.
//!
//! **Anything else** (`/extra`) is one more call: what the person wrote
//! becomes proposed changes across the profile (summary, headline, skills,
//! background, goals, facts on a named position), each with their own words
//! as its quote. A claim their words do not say is a flag, shown, never
//! dropped: *It's true*, *Fix* or *Use my own words*.
//!
//! **Nothing reaches the profile until Save** (`/save`), which applies every
//! pending change and every confirmed assumption in one write: one new
//! profile version, so the strength and the gap review refresh. Assumptions
//! the person never confirmed are not saved. Pending state lives seven days.
//!
//! Employers are read through the [`crate::ports::EmployerResearch`] port: the
//! latest five, only where a site is known, cached 30 days, no loop and no
//! open-web search. Every call counts toward [`DAILY_CAP`] and the AI budget;
//! nothing runs in the background.

pub mod judge;
pub mod prompt;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashSet};

use applicamp_ai::{Caller, ChatRequest, Gateway, Message, ResponseFormat};
use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use chrono::{DateTime, Datelike as _, TimeDelta, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use self::judge::{
    Change, Field, Flag, RawChanges, RawPortrait, Resolution, Sentence, SentenceKind,
};
use crate::{
    events::GoalsUpdated,
    features::{
        completion::{self, current},
        grill::{self, prompt::language},
        imports::{Ai, ExpireImports},
    },
    goals::Goals,
    ports::{FactBasis, ProposedFact, Researcher},
    schema::{Area, FactStatus, Profile, ProfileFact, Skill},
    strength,
};

/// Hard cap on the wizard's model calls per account per UTC day.
pub const DAILY_CAP: i64 = 30;
/// Longest "Anything else", in characters.
pub const MAX_EXTRA: usize = 4_000;
/// Most past employers researched (the latest).
pub const MAX_EMPLOYERS: usize = 5;
/// How long pending state lives after the last touch.
pub const PENDING_LIFETIME: TimeDelta = TimeDelta::days(7);
/// Answer budgets in tokens: the first attempt, and the one retry after a
/// cut-off (a reasoning model thinks first, and thinking counts).
const BUDGETS: [u32; 2] = [4_000, 8_000];

// ---- views ----------------------------------------------------------------

/// What the person said about an assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// "That's right": a confirmed fact when saved.
    Right,
    /// "That's wrong": struck out, never saved, not suggested again.
    Wrong,
    /// "Ask me later": kept for the questions that follow.
    Later,
}

/// The person's answer to "Is this accurate?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Accuracy {
    /// "Yes, mostly".
    Yes,
    /// "Some of it's wrong".
    Some,
    /// "Not really".
    No,
}

impl Accuracy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Some => "some",
            Self::No => "no",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "yes" => Some(Self::Yes),
            "some" => Some(Self::Some),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// A sentence of the portrait.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SentenceView {
    /// Its id.
    pub id: String,
    /// What it says.
    pub text: String,
    /// Where it comes from.
    pub kind: SentenceKind,
    /// For an assumption, why it was assumed (the person's language).
    pub why: Option<String>,
    /// The check could not find it in the profile in these words, so it
    /// became an assumption.
    pub downgraded: bool,
    /// What it is about.
    pub topic: Area,
    /// The position it is about (its place in `work`).
    pub position: Option<u32>,
    /// The employer pages it was read on.
    pub urls: Vec<String>,
    /// Told just now, from "Anything else".
    pub new: bool,
    /// What the person said about the assumption.
    pub choice: Option<Choice>,
}

/// What reading an employer came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmployerState {
    /// Its own site was read (or its facts are fresh).
    Read,
    /// Its site is known but the portrait was made before it was read.
    Unread,
    /// No site is known, so nothing was fetched.
    NoSite,
    /// It could not be read (see `error`).
    Failed,
}

/// A past employer, as researched.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmployerView {
    /// The latest position there (its place in `work`).
    pub position: u32,
    /// The company.
    pub company: String,
    /// Its own site, when known.
    pub site: Option<String>,
    /// What came of it.
    pub state: EmployerState,
    /// Facts found on the site.
    pub facts: u32,
    /// For a failed read, the problem code (`robots_disallows`, …).
    pub error: Option<String>,
}

/// A pending change, as the person sees it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChangeView {
    /// Its id.
    pub id: String,
    /// What it changes.
    pub field: Field,
    /// The new value.
    pub value: String,
    /// For a fact: the position it is about.
    pub position: Option<u32>,
    /// Their words that support it.
    pub quote: String,
    /// What is wrong with it, if anything: shown, not dropped.
    pub flags: Vec<Flag>,
    /// How they settled the flags.
    pub resolution: Option<Resolution>,
}

impl From<&Change> for ChangeView {
    fn from(change: &Change) -> Self {
        Self {
            id: change.id.clone(),
            field: change.field,
            value: change.value.clone(),
            position: change.position,
            quote: change.quote.clone(),
            flags: change.flags.clone(),
            resolution: change.resolution,
        }
    }
}

/// The wizard's state.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AboutView {
    /// There is a profile to write about.
    pub has_profile: bool,
    /// The profile's version now.
    pub profile_version: i32,
    /// A portrait was made for this very version.
    pub built: bool,
    /// The portrait: sentences from the profile, what the person told, and
    /// the assumptions, in the order they read; then what they just added.
    pub sentences: Vec<SentenceView>,
    /// The latest employers and what reading them came to.
    pub employers: Vec<EmployerView>,
    /// "Is this accurate?".
    pub accuracy: Option<Accuracy>,
    /// What the person wrote under "Anything else".
    pub extra: Option<String>,
    /// The changes proposed from it.
    pub changes: Vec<ChangeView>,
    /// Model calls made today by this account, of `daily_cap`.
    pub calls_today: u32,
    /// The most model calls a wizard can make for an account in a UTC day.
    pub daily_cap: u32,
    /// The longest "Anything else", in characters.
    pub extra_max: u32,
}

// ---- storage --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Row {
    profile_version: Option<i32>,
    portrait: sqlx::types::Json<Vec<Sentence>>,
    employers: sqlx::types::Json<Vec<EmployerView>>,
    choices: sqlx::types::Json<BTreeMap<String, Choice>>,
    accuracy: Option<String>,
    extra: Option<String>,
    changes: sqlx::types::Json<Vec<Change>>,
}

impl Row {
    fn empty() -> Self {
        Self {
            profile_version: None,
            portrait: sqlx::types::Json(Vec::new()),
            employers: sqlx::types::Json(Vec::new()),
            choices: sqlx::types::Json(BTreeMap::new()),
            accuracy: None,
            extra: None,
            changes: sqlx::types::Json(Vec::new()),
        }
    }
}

async fn row_of(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "SELECT profile_version, portrait, employers, choices, accuracy, extra, changes
           FROM profile.about_you WHERE account_id = $1 AND expires_at > $2",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(db)
    .await?)
}

/// The row, locked for this transaction; none if there is none (or it has
/// expired).
async fn lock(tx: &mut PgConnection, account: Uuid, now: DateTime<Utc>) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "SELECT profile_version, portrait, employers, choices, accuracy, extra, changes
           FROM profile.about_you WHERE account_id = $1 AND expires_at > $2 FOR UPDATE",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?)
}

async fn put(tx: &mut PgConnection, account: Uuid, row: &Row, now: DateTime<Utc>) -> Result<()> {
    let expires_at = now + PENDING_LIFETIME;
    sqlx::query(
        "INSERT INTO profile.about_you
            (account_id, profile_version, portrait, employers, choices, accuracy, extra, changes,
             created_at, updated_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10)
         ON CONFLICT (account_id) DO UPDATE SET
            profile_version = excluded.profile_version, portrait = excluded.portrait,
            employers = excluded.employers, choices = excluded.choices,
            accuracy = excluded.accuracy, extra = excluded.extra, changes = excluded.changes,
            updated_at = excluded.updated_at, expires_at = excluded.expires_at",
    )
    .bind(account)
    .bind(row.profile_version)
    .bind(&row.portrait)
    .bind(&row.employers)
    .bind(&row.choices)
    .bind(&row.accuracy)
    .bind(&row.extra)
    .bind(&row.changes)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue_at(&mut *tx, &ExpireImports {}, expires_at).await?;
    Ok(())
}

/// Model calls `account` made since the start of the UTC day.
async fn calls_today(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM profile.about_calls WHERE account_id = $1 AND created_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(db)
    .await?)
}

/// Whether `calls` already made today leave no room for `more`.
pub const fn over_cap(calls: i64, more: i64) -> bool {
    calls + more > DAILY_CAP
}

fn cap_reached() -> Error {
    Error::TooManyRequests(
        "about_daily_cap",
        format!("This is limited to {DAILY_CAP} AI steps a day. It continues tomorrow."),
    )
}

async fn record_call(db: &PgPool, account: Uuid, kind: &str, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "INSERT INTO profile.about_calls (id, account_id, kind, created_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(kind)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

fn counted(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn gateway(ai: &Ai) -> Result<&Gateway> {
    match (&ai.0, ai.can_structure()) {
        (Some(gateway), true) => Ok(gateway),
        _ => Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    }
}

fn needs_profile() -> Error {
    Error::Conflict(
        "profile_needed",
        "Add your profile first (import it or write it), then tell me about you.".to_owned(),
    )
}

/// Ask `system` / `user` for a JSON object shaped like `schema`: one call, a
/// second only after a cut-off. A spending limit ends it at once.
#[allow(clippy::too_many_arguments)]
async fn ask<T: DeserializeOwned>(
    gateway: &Gateway,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    name: &str,
    schema: &Value,
    system: &str,
    user: &str,
) -> Result<T> {
    let caller = Caller { db, account, now };
    let mut last = Error::Unavailable(applicamp_ai::AI_FAILED);
    for max_tokens in BUDGETS {
        let request = ChatRequest {
            temperature: Some(0.0),
            max_tokens: Some(max_tokens),
            response_format: Some(ResponseFormat::json_schema(name, schema.clone())),
            ..ChatRequest::new(vec![Message::system(system), Message::user(user)])
        };
        let reply = gateway.complete(caller, request).await?;
        if reply.finish_reason.as_deref() == Some("length") {
            tracing::warn!(max_tokens, call = name, "an answer was cut off");
            continue;
        }
        match reply.json::<T>() {
            Ok(read) => return Ok(read),
            Err(error) => {
                tracing::warn!(%error, call = name, "an answer was unusable");
                last = Error::from(error);
            }
        }
    }
    Err(last)
}

// ---- the view -------------------------------------------------------------

/// The latest employers as they are now: stored results where the portrait
/// was made for the same company, else what is known about the site.
async fn employers_of(
    state: &AppState,
    research: &Researcher,
    account: Uuid,
    profile: &Profile,
    stored: &[EmployerView],
) -> Result<Vec<EmployerView>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (index, work) in profile.work.iter().enumerate() {
        let Some(company) = work
            .name
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        else {
            continue;
        };
        if out.len() >= MAX_EMPLOYERS {
            break;
        }
        if !seen.insert(applicamp_kernel::claims::squash(company)) {
            continue;
        }
        let site = match research.0.as_deref() {
            Some(port) => port.known_site(&state.db, account, company).await?,
            None => None,
        };
        let before = stored.iter().find(|e| e.company == company);
        let (state_of, facts, error) = match (before, &site) {
            (Some(b), Some(_))
                if b.state == EmployerState::Read || b.state == EmployerState::Failed =>
            {
                (b.state, b.facts, b.error.clone())
            }
            (_, Some(_)) => (EmployerState::Unread, 0, None),
            (_, None) => (EmployerState::NoSite, 0, None),
        };
        out.push(EmployerView {
            position: u32::try_from(index).map_err(Error::internal)?,
            company: company.to_owned(),
            site,
            state: state_of,
            facts,
            error,
        });
    }
    Ok(out)
}

/// The sentences the person told: one per change except skills.
fn told_sentences(changes: &[Change]) -> Vec<SentenceView> {
    changes
        .iter()
        .filter(|c| c.field != Field::Skill)
        .map(|c| SentenceView {
            id: format!("t{}", c.id),
            text: c.sentence.clone(),
            kind: SentenceKind::Told,
            why: None,
            downgraded: false,
            topic: c.area,
            position: c.position,
            urls: Vec::new(),
            new: true,
            choice: None,
        })
        .collect()
}

fn sentence_view(sentence: &Sentence, choices: &BTreeMap<String, Choice>) -> SentenceView {
    SentenceView {
        id: sentence.id.clone(),
        text: sentence.text.clone(),
        kind: sentence.kind,
        why: sentence.why.clone(),
        downgraded: sentence.downgraded,
        topic: sentence.topic,
        position: sentence.position,
        urls: sentence.urls.clone(),
        new: sentence.new,
        choice: choices.get(&sentence.id).copied(),
    }
}

async fn view_of(
    state: &AppState,
    research: &Researcher,
    account: Uuid,
    profile: Option<&completion::Current>,
    row: Option<&Row>,
) -> Result<AboutView> {
    let now = state.clock.now();
    let empty = Row::empty();
    let row = row.unwrap_or(&empty);
    let version = profile.map_or(0, |p| p.version);
    let built = profile.is_some() && row.profile_version == Some(version);
    let employers = match profile {
        Some(p) => employers_of(state, research, account, &p.profile, &row.employers.0).await?,
        None => Vec::new(),
    };
    let mut sentences = Vec::new();
    if built {
        sentences.extend(
            row.portrait
                .0
                .iter()
                .map(|s| sentence_view(s, &row.choices.0)),
        );
    }
    sentences.extend(told_sentences(&row.changes.0));
    Ok(AboutView {
        has_profile: profile.is_some(),
        profile_version: version,
        built,
        sentences,
        employers,
        accuracy: row.accuracy.as_deref().and_then(Accuracy::parse),
        extra: row.extra.clone(),
        changes: row.changes.0.iter().map(ChangeView::from).collect(),
        calls_today: counted(calls_today(&state.db, account, now).await?),
        daily_cap: counted(DAILY_CAP),
        extra_max: u32::try_from(MAX_EXTRA).unwrap_or(u32::MAX),
    })
}

async fn respond(
    state: &AppState,
    research: &Researcher,
    account: Uuid,
) -> Result<Json<AboutView>> {
    let seen = current(&state.db, account).await?;
    let row = row_of(&state.db, account, state.clock.now()).await?;
    Ok(Json(
        view_of(state, research, account, seen.as_ref(), row.as_ref()).await?,
    ))
}

// ---- requests ---------------------------------------------------------------

/// The language of what the model writes, and whether to rebuild.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PortraitQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
    /// Make the portrait again, even though one exists for this version
    /// (after an employer's website was added).
    again: Option<bool>,
}

/// The language of what the model writes.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LocaleQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
}

/// What the person says about an assumption.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoiceRequest {
    /// Left out (or null), the choice is taken back.
    pub choice: Option<Choice>,
}

/// "Is this accurate?".
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccuracyRequest {
    pub accuracy: Accuracy,
}

/// An employer's own website, typed.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SiteRequest {
    /// An http or https address; only its host (and port) is kept.
    pub url: String,
}

/// "Anything else I should know about you?".
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtraRequest {
    /// In their own words, 1 to 4000 characters.
    pub text: String,
}

/// What the person does with a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    /// "It's true": they stand behind what is flagged.
    True,
    /// "Fix" / Edit: a new value, checked again against their words.
    Edit,
    /// "Use my own words": the exact text they wrote.
    OwnWords,
}

/// What the person does with a change.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeRequest {
    pub action: ChangeAction,
    /// For `edit`: the new value.
    pub value: Option<String>,
}

fn invalid(code: &'static str, detail: impl Into<String>) -> Error {
    Error::BadRequest(code, detail.into())
}

// ---- endpoints ------------------------------------------------------------

/// The wizard's state: the portrait if one was made for this version of the
/// profile, the latest employers, what the person said, and the pending
/// changes. Never calls the model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/about",
    tag = "profile",
    operation_id = "profile.about",
    security(("session" = [])),
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
) -> Result<Json<AboutView>> {
    respond(&state, &research, principal.account_id).await
}

/// Make the portrait, or take the one made for this profile version.
///
/// Reads the latest five employers' own websites first (only where a site is
/// known; cached 30 days), then makes the portrait: **one model call** per
/// profile version, plus one per employer read. The model never sees the
/// person's name or contact details. Every `known` or `told` sentence is
/// checked against the sources it cites; one that fails is downgraded to
/// `assumed`, never dropped.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/portrait",
    tag = "profile",
    operation_id = "profile.about_portrait",
    security(("session" = [])),
    params(PortraitQuery),
    responses(
        (status = 200, description = "The wizard's state, with the portrait.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`about_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn portrait(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Extension(research): Extension<Researcher>,
    Query(query): Query<PortraitQuery>,
) -> Result<Json<AboutView>> {
    let account = principal.account_id;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(needs_profile)?;
    let now = state.clock.now();
    let before = row_of(&state.db, account, now).await?;
    if before
        .as_ref()
        .is_some_and(|r| r.profile_version == Some(seen.version))
        && query.again != Some(true)
    {
        return respond(&state, &research, account).await;
    }
    let gateway = gateway(&ai)?;
    if over_cap(calls_today(&state.db, account, now).await?, 1) {
        return Err(cap_reached());
    }

    // The employers first: their facts are cached, so a rebuild reads nothing
    // twice.
    let stored: Vec<EmployerView> = before
        .as_ref()
        .map_or_else(Vec::new, |r| r.employers.0.clone());
    let mut employers = employers_of(&state, &research, account, &seen.profile, &stored).await?;
    let mut found: Vec<(usize, Vec<ProposedFact>)> = Vec::new();
    if let Some(port) = research.0.as_deref() {
        for employer in &mut employers {
            if employer.site.is_none() {
                continue;
            }
            if over_cap(calls_today(&state.db, account, now).await?, 2) {
                employer.state = EmployerState::Failed;
                employer.error = Some("about_daily_cap".to_owned());
                continue;
            }
            match port.run(&state, account, &employer.company, now).await {
                Ok(result) => {
                    if result.read {
                        record_call(&state.db, account, "research", now).await?;
                    }
                    let facts: Vec<ProposedFact> = result
                        .facts
                        .into_iter()
                        .map(ProposedFact::checked)
                        .collect();
                    employer.site = result.site.or(employer.site.take());
                    employer.state = EmployerState::Read;
                    employer.facts = counted(
                        i64::try_from(
                            facts
                                .iter()
                                .filter(|f| f.status == FactBasis::Sourced)
                                .count(),
                        )
                        .unwrap_or(0),
                    );
                    employer.error = None;
                    found.push((employer.position as usize, facts));
                }
                // A spending limit ends the whole step, in plain words.
                Err(error) if error.code().starts_with("ai_") => return Err(error),
                Err(error) => {
                    employer.state = EmployerState::Failed;
                    employer.error = Some(error.code().to_owned());
                }
            }
        }
    }

    let sources = prompt::sources_of(&seen.profile, &seen.goals, &found, now.year());
    let role_keys: Vec<String> = seen.profile.work.iter().map(grill::role_key).collect();
    let message = prompt::portrait_message(
        &sources,
        &prompt::roles_of(&seen.profile),
        language(query.locale.as_deref()).as_deref(),
        &now.date_naive().to_string(),
    );
    let raw: RawPortrait = ask(
        gateway,
        &state.db,
        account,
        now,
        "profile_about_portrait",
        &prompt::portrait_schema(),
        prompt::PORTRAIT_INSTRUCTIONS,
        &message,
    )
    .await?;
    record_call(&state.db, account, "portrait", now).await?;
    let judged = judge::judge_portrait(raw, &sources, &role_keys);
    // Ids and counts only: never the profile or the sentences.
    tracing::info!(
        prompt = prompt::PROMPT_VERSION,
        sentences = judged.sentences.len(),
        downgraded = judged.downgraded,
        employers = employers.len(),
        "an about-you portrait made"
    );

    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now)
        .await?
        .unwrap_or_else(Row::empty);
    row.profile_version = Some(seen.version);
    row.portrait = sqlx::types::Json(judged.sentences);
    row.employers = sqlx::types::Json(employers);
    // Choices belong to the portrait they were made on.
    row.choices = sqlx::types::Json(BTreeMap::new());
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

/// Say what you think of an assumption: *That's right*, *That's wrong* or
/// *Ask me later*. Nothing is saved yet.
#[utoipa::path(
    put,
    path = "/api/v1/profile/about/sentences/{id}",
    tag = "profile",
    operation_id = "profile.about_choose",
    security(("session" = [])),
    params(("id" = String, Path, description = "The assumption's id in the portrait.")),
    request_body = ChoiceRequest,
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 400, description = "`invalid_choice`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such assumption", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn choose(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    Path(id): Path<String>,
    body: std::result::Result<Json<ChoiceRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(ChoiceRequest { choice }) =
        body.map_err(|rejection| invalid("invalid_choice", rejection.body_text()))?;
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now).await?.ok_or(Error::NotFound)?;
    if !row
        .portrait
        .0
        .iter()
        .any(|s| s.id == id && s.kind == SentenceKind::Assumed)
    {
        return Err(Error::NotFound);
    }
    match choice {
        Some(choice) => {
            row.choices.0.insert(id, choice);
        }
        None => {
            row.choices.0.remove(&id);
        }
    }
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

/// "Is this accurate?": stored; the questions that follow use it to decide
/// how many to ask.
#[utoipa::path(
    put,
    path = "/api/v1/profile/about/accuracy",
    tag = "profile",
    operation_id = "profile.about_accuracy",
    security(("session" = [])),
    request_body = AccuracyRequest,
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 400, description = "`invalid_accuracy`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn accuracy(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    body: std::result::Result<Json<AccuracyRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(AccuracyRequest { accuracy }) =
        body.map_err(|rejection| invalid("invalid_accuracy", rejection.body_text()))?;
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now)
        .await?
        .unwrap_or_else(Row::empty);
    row.accuracy = Some(accuracy.as_str().to_owned());
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

/// Type a past employer's own website. Nothing is read until the portrait is
/// made again (`POST …/portrait?again=true`).
#[utoipa::path(
    put,
    path = "/api/v1/profile/about/employers/{position}/site",
    tag = "profile",
    operation_id = "profile.about_employer_site",
    security(("session" = [])),
    params(("position" = u32, Path, description = "A position of the profile's `work` list.")),
    request_body = SiteRequest,
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 400, description = "`invalid_site`; `site_not_allowed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such position", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`research_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn employer_site(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    Path(position): Path<u32>,
    body: std::result::Result<Json<SiteRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(SiteRequest { url }) =
        body.map_err(|rejection| invalid("invalid_site", rejection.body_text()))?;
    let account = principal.account_id;
    let seen = current(&state.db, account).await?.ok_or(Error::NotFound)?;
    let work = usize::try_from(position)
        .ok()
        .and_then(|i| seen.profile.work.get(i))
        .ok_or(Error::NotFound)?;
    let company = work
        .name
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or(Error::NotFound)?;
    let port = research
        .0
        .as_deref()
        .ok_or(Error::Unavailable("research_unavailable"))?;
    port.set_site(&state, account, company, &url).await?;
    respond(&state, &research, account).await
}

/// Turn what the person wrote into proposed changes to the profile.
///
/// **One model call.** Each change quotes their words; a claim their words
/// do not say, or a quote that is not theirs, is a flag, shown and never
/// dropped. The changes also add sentences to the portrait, as *told*. A new
/// submission replaces the changes the last one made. Nothing is saved yet.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/extra",
    tag = "profile",
    operation_id = "profile.about_extra",
    security(("session" = [])),
    params(LocaleQuery),
    request_body = ExtraRequest,
    responses(
        (status = 200, description = "The wizard's state, with the changes.", body = AboutView),
        (status = 400, description = "`invalid_text`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`about_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn extra(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Extension(research): Extension<Researcher>,
    Query(query): Query<LocaleQuery>,
    body: std::result::Result<Json<ExtraRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(ExtraRequest { text }) =
        body.map_err(|rejection| invalid("invalid_text", rejection.body_text()))?;
    let text = text.trim().to_owned();
    if text.is_empty() || text.chars().count() > MAX_EXTRA {
        return Err(invalid(
            "invalid_text",
            format!("Write between 1 and {MAX_EXTRA} characters."),
        ));
    }
    let account = principal.account_id;
    let gateway = gateway(&ai)?;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(needs_profile)?;
    let now = state.clock.now();
    if over_cap(calls_today(&state.db, account, now).await?, 1) {
        return Err(cap_reached());
    }
    let message = prompt::extra_message(
        &seen.profile,
        &seen.goals,
        &text,
        language(query.locale.as_deref()).as_deref(),
    );
    let raw: RawChanges = ask(
        gateway,
        &state.db,
        account,
        now,
        "profile_about_extra",
        &prompt::extra_schema(),
        prompt::EXTRA_INSTRUCTIONS,
        &message,
    )
    .await?;
    record_call(&state.db, account, "extra", now).await?;
    let role_keys: Vec<String> = seen.profile.work.iter().map(grill::role_key).collect();
    let changes = judge::judge_changes(raw, &text, &role_keys);
    tracing::info!(
        prompt = prompt::PROMPT_VERSION,
        changes = changes.len(),
        flagged = changes.iter().filter(|c| !c.flags.is_empty()).count(),
        "an about-you text read"
    );
    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now)
        .await?
        .unwrap_or_else(Row::empty);
    row.extra = Some(text);
    row.changes = sqlx::types::Json(changes);
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

/// Settle a change: *It's true*, *Fix* (or *Edit*) with a new value that is
/// checked again against their words, or *Use my own words*, which puts the
/// exact text they wrote into the field (for the summary, their whole
/// answer). Nothing is saved yet.
#[utoipa::path(
    put,
    path = "/api/v1/profile/about/changes/{id}",
    tag = "profile",
    operation_id = "profile.about_change",
    security(("session" = [])),
    params(("id" = String, Path, description = "The change.")),
    request_body = ChangeRequest,
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 400, description = "`invalid_change`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn change(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    Path(id): Path<String>,
    body: std::result::Result<Json<ChangeRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(request) =
        body.map_err(|rejection| invalid("invalid_change", rejection.body_text()))?;
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now).await?.ok_or(Error::NotFound)?;
    let text = row.extra.clone().unwrap_or_default();
    let change = row
        .changes
        .0
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(Error::NotFound)?;
    match request.action {
        ChangeAction::True => change.resolution = Some(Resolution::True),
        ChangeAction::Edit => {
            let value = judge::cut(
                request.value.as_deref().unwrap_or_default(),
                change.field.max(),
            );
            if value.is_empty() {
                return Err(invalid("invalid_change", "Write what it should say."));
            }
            change.flags = judge::flags_of(&text, &value, &change.quote);
            change.sentence = judge::cut(&value, judge::MAX_SENTENCE);
            change.value = value;
            change.resolution = change.flags.is_empty().then_some(Resolution::Fixed);
        }
        ChangeAction::OwnWords => {
            let value = judge::own_words(change, &text);
            change.sentence = judge::cut(&value, judge::MAX_SENTENCE);
            change.value = value;
            change.flags = Vec::new();
            change.resolution = Some(Resolution::OwnWords);
        }
    }
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

/// Take a change out of what will be saved.
#[utoipa::path(
    delete,
    path = "/api/v1/profile/about/changes/{id}",
    tag = "profile",
    operation_id = "profile.about_remove_change",
    security(("session" = [])),
    params(("id" = String, Path, description = "The change.")),
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn remove_change(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    Path(id): Path<String>,
) -> Result<Json<AboutView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = lock(&mut tx, account, now).await?.ok_or(Error::NotFound)?;
    let before = row.changes.0.len();
    row.changes.0.retain(|c| c.id != id);
    if row.changes.0.len() == before {
        return Err(Error::NotFound);
    }
    put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    respond(&state, &research, account).await
}

// ---- review and save --------------------------------------------------------

/// Which part of the profile a group of rows changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupKey {
    Summary,
    Headline,
    Skills,
    /// A position (see `position`).
    Position,
    Background,
    Goals,
}

/// Where a row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RowSource {
    /// Something they wrote.
    Told,
    /// An assumption they confirmed.
    Confirmed,
}

/// One line of what will be saved.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReviewRow {
    /// A change's id, or an assumption's sentence id.
    pub id: String,
    pub source: RowSource,
    pub text: String,
    /// For a change, the field.
    pub field: Option<Field>,
    pub flags: Vec<Flag>,
    pub resolution: Option<Resolution>,
}

/// The rows that change one part of the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReviewGroup {
    pub key: GroupKey,
    /// For a position: its place in `work`.
    pub position: Option<u32>,
    /// For a position: its title and company, as the profile has them.
    pub title: Option<String>,
    pub rows: Vec<ReviewRow>,
}

/// What Save would do.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AboutReview {
    /// The portrait as it will stand: what the profile says, what they told,
    /// and the assumptions they confirmed. The rest is left out.
    pub sentences: Vec<SentenceView>,
    pub groups: Vec<ReviewGroup>,
    /// Flags still waiting for the person: Save waits for them.
    pub open_flags: u32,
    /// Assumptions left unconfirmed (not saved).
    pub left_out: u32,
}

fn position_title(profile: &Profile, index: usize) -> String {
    let work = &profile.work[index];
    match (work.name.as_deref(), work.position.as_deref()) {
        (Some(c), Some(p)) => format!("{c} · {p}"),
        (Some(x), None) | (None, Some(x)) => x.to_owned(),
        (None, None) => String::new(),
    }
}

/// The position of `profile` a pending item is about: by its key, else the
/// latest.
fn place_of(profile: &Profile, key: Option<&str>) -> Option<usize> {
    if profile.work.is_empty() {
        return None;
    }
    match key {
        Some(key) => profile.work.iter().position(|w| grill::role_key(w) == key),
        None => Some(0),
    }
}

fn review_of(profile: &Profile, row: &Row) -> AboutReview {
    let mut groups: Vec<ReviewGroup> = Vec::new();
    let mut group = |key: GroupKey, position: Option<usize>| -> usize {
        let at = groups.iter().position(|g| {
            g.key == key && g.position == position.map(|p| u32::try_from(p).unwrap_or(0))
        });
        at.unwrap_or_else(|| {
            groups.push(ReviewGroup {
                key,
                position: position.map(|p| u32::try_from(p).unwrap_or(0)),
                title: position.map(|p| position_title(profile, p)),
                rows: Vec::new(),
            });
            groups.len() - 1
        })
    };
    let mut rows: Vec<(usize, ReviewRow)> = Vec::new();
    for change in &row.changes.0 {
        let (key, position) = match change.field {
            Field::Summary => (GroupKey::Summary, None),
            Field::Headline => (GroupKey::Headline, None),
            Field::Skill => (GroupKey::Skills, None),
            Field::Background => (GroupKey::Background, None),
            Field::RoleFact => match place_of(profile, change.role_key.as_deref()) {
                Some(p) => (GroupKey::Position, Some(p)),
                None => continue,
            },
            _ => (GroupKey::Goals, None),
        };
        rows.push((
            group(key, position),
            ReviewRow {
                id: change.id.clone(),
                source: RowSource::Told,
                text: change.value.clone(),
                field: Some(change.field),
                flags: change.flags.clone(),
                resolution: change.resolution,
            },
        ));
    }
    let mut left_out = 0;
    let mut sentences = Vec::new();
    for sentence in &row.portrait.0 {
        let choice = row.choices.0.get(&sentence.id).copied();
        if sentence.kind != SentenceKind::Assumed {
            sentences.push(sentence_view(sentence, &row.choices.0));
            continue;
        }
        if choice != Some(Choice::Right) {
            left_out += 1;
            continue;
        }
        sentences.push(sentence_view(sentence, &row.choices.0));
        if let Some(position) = place_of(profile, sentence.role_key.as_deref()) {
            rows.push((
                group(GroupKey::Position, Some(position)),
                ReviewRow {
                    id: sentence.id.clone(),
                    source: RowSource::Confirmed,
                    text: sentence.text.clone(),
                    field: None,
                    flags: Vec::new(),
                    resolution: None,
                },
            ));
        }
    }
    sentences.extend(told_sentences(&row.changes.0));
    for (at, entry) in rows {
        groups[at].rows.push(entry);
    }
    // Summary, headline, skills, positions (newest first), background, goals.
    let order = |g: &ReviewGroup| match g.key {
        GroupKey::Summary => 0,
        GroupKey::Headline => 1,
        GroupKey::Skills => 2,
        GroupKey::Position => 3,
        GroupKey::Background => 4,
        GroupKey::Goals => 5,
    };
    groups.sort_by_key(|g| (order(g), g.position));
    let open_flags = row.changes.0.iter().filter(|c| c.open()).count();
    AboutReview {
        sentences,
        groups,
        open_flags: counted(i64::try_from(open_flags).unwrap_or(0)),
        left_out,
    }
}

/// What Save would do: every pending change and confirmed assumption,
/// grouped by the part of the profile it changes, and the portrait as it
/// will stand. Never calls the model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/about/review",
    tag = "profile",
    operation_id = "profile.about_review",
    security(("session" = [])),
    responses(
        (status = 200, description = "What would be saved.", body = AboutReview),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn review(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<AboutReview>> {
    let account = principal.account_id;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(needs_profile)?;
    let row = row_of(&state.db, account, state.clock.now())
        .await?
        .unwrap_or_else(Row::empty);
    Ok(Json(review_of(&seen.profile, &row)))
}

/// What Save did.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Saved {
    /// The profile's new version.
    pub profile_version: i32,
    /// How strong the profile is now, 0 to 100.
    pub strength: u8,
    /// Changes and confirmed assumptions written.
    pub saved: u32,
    /// Items that no longer fit the profile (a position that was removed, a
    /// list that is full) and were left out.
    pub skipped: u32,
}

/// Apply `row` to `profile` and `goals`: the number written and the number
/// that no longer fit.
fn apply(
    row: &Row,
    profile: &mut Profile,
    goals: &mut Goals,
    now: DateTime<Utc>,
) -> (u32, u32, bool) {
    let (mut saved, mut skipped, mut goals_changed) = (0, 0, false);
    for change in &row.changes.0 {
        let value = change.value.trim().to_owned();
        let ok = match change.field {
            Field::Summary => {
                profile.basics.summary = Some(value);
                true
            }
            Field::Headline => {
                profile.basics.label = Some(value);
                true
            }
            Field::Background => {
                profile.basics.background = Some(value);
                true
            }
            Field::Skill => {
                if !profile
                    .skills
                    .iter()
                    .any(|s| judge::same(Field::Skill, &s.name, &value))
                {
                    profile.skills.push(Skill {
                        name: value,
                        level: None,
                    });
                }
                true
            }
            Field::GoalRole | Field::GoalAvoid | Field::GoalMustHave | Field::GoalLocation => {
                let list = match change.field {
                    Field::GoalRole => &mut goals.roles,
                    Field::GoalAvoid => &mut goals.avoid,
                    Field::GoalMustHave => &mut goals.must_have,
                    _ => &mut goals.locations,
                };
                if list.iter().any(|g| judge::same(change.field, g, &value)) {
                    true
                } else if judge::goal_room(change.field, list.len()) {
                    list.push(value);
                    goals_changed = true;
                    true
                } else {
                    false
                }
            }
            Field::RoleFact => place_of(profile, change.role_key.as_deref())
                .and_then(|at| profile.work.get_mut(at))
                .is_some_and(|work| {
                    push_fact(
                        work,
                        &value,
                        &change.quote,
                        change.area,
                        FactStatus::Told,
                        None,
                        now,
                    );
                    true
                }),
        };
        if ok {
            saved += 1;
        } else {
            skipped += 1;
        }
    }
    for sentence in &row.portrait.0 {
        if sentence.kind != SentenceKind::Assumed
            || row.choices.0.get(&sentence.id) != Some(&Choice::Right)
        {
            continue;
        }
        let basis = sentence
            .why
            .clone()
            .or_else(|| sentence.urls.first().cloned());
        let placed = place_of(profile, sentence.role_key.as_deref())
            .and_then(|at| profile.work.get_mut(at))
            .is_some_and(|work| {
                push_fact(
                    work,
                    &sentence.text,
                    &sentence.text,
                    sentence.topic,
                    FactStatus::ConfirmedAssumption,
                    basis,
                    now,
                );
                true
            });
        if placed {
            saved += 1;
        } else {
            skipped += 1;
        }
    }
    (saved, skipped, goals_changed)
}

fn push_fact(
    work: &mut crate::schema::Work,
    text: &str,
    quote: &str,
    area: Area,
    status: FactStatus,
    basis: Option<String>,
    now: DateTime<Utc>,
) {
    let key = applicamp_kernel::claims::squash(text);
    if work
        .facts
        .iter()
        .any(|f| applicamp_kernel::claims::squash(&f.text) == key)
    {
        return;
    }
    work.facts.push(ProfileFact {
        id: Uuid::now_v7().to_string(),
        text: text.to_owned(),
        quote: quote.to_owned(),
        area,
        kept_at: now,
        status,
        basis,
    });
}

/// Save everything, atomically.
///
/// Applies every pending change and every confirmed assumption in **one
/// write** of the profile (and of the goals, when they changed): one new
/// profile version, so the strength and the gap review refresh. Assumptions
/// that were never confirmed are not saved. A flag still waiting for the
/// person stops it (`changes_flagged`). Afterwards the pending state is gone.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/save",
    tag = "profile",
    operation_id = "profile.about_save",
    security(("session" = [])),
    responses(
        (status = 200, description = "Saved.", body = Saved),
        (status = 400, description = "`invalid_profile`; `invalid_goals`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: nothing is pending", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`; `changes_flagged`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn save(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<Saved>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let row = lock(&mut tx, account, now).await?.ok_or(Error::NotFound)?;
    if row.changes.0.iter().any(Change::open) {
        return Err(Error::Conflict(
            "changes_flagged",
            "Some of what you wrote has a claim I can't find in your words. Settle it first."
                .to_owned(),
        ));
    }
    // The profile as it is now, locked: a hand edit in between is kept.
    let stored: Option<(sqlx::types::Json<Profile>,)> =
        sqlx::query_as("SELECT data FROM profile.profiles WHERE account_id = $1 FOR UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    let mut profile = stored.ok_or_else(needs_profile)?.0.0;
    let goals_row: Option<(sqlx::types::Json<Goals>,)> =
        sqlx::query_as("SELECT answers FROM profile.goals WHERE account_id = $1 FOR UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    let mut goals = goals_row.map_or_else(Goals::default, |g| g.0.0);

    let (saved, skipped, goals_changed) = apply(&row, &mut profile, &mut goals, now);
    let profile = profile.tidied();
    profile.validate()?;
    let version =
        completion::write_profile(&state, &mut tx, account, &profile, Some("answer")).await?;
    if goals_changed {
        let goals = goals.clone().tidied();
        goals.validate()?;
        let goals_version: i32 = sqlx::query_scalar(
            "INSERT INTO profile.goals (account_id, version, answers, updated_at)
             VALUES ($1, 1, $2, $3)
             ON CONFLICT (account_id) DO UPDATE
             SET version = goals.version + 1, answers = excluded.answers,
                 updated_at = excluded.updated_at
             RETURNING version",
        )
        .bind(account)
        .bind(sqlx::types::Json(&goals))
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        state
            .events
            .publish(
                &mut tx,
                &GoalsUpdated {
                    account,
                    version: goals_version,
                },
            )
            .await?;
    }
    sqlx::query("DELETE FROM profile.about_you WHERE account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(saved, skipped, "an about-you saved");
    Ok(Json(Saved {
        profile_version: version,
        strength: strength::compute(&profile, &goals).score,
        saved,
        skipped,
    }))
}
