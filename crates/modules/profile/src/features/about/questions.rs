//! The questions about the gaps (#44), one at a time, each saying why.
//!
//! [`super::gaps`] decides what to ask, in plain code. Here the model does
//! two small things and nothing else:
//!
//! - **Next** words one question about the top gap (1 call). It is shown
//!   only that gap's context: the position, its facts and the assumption it
//!   concerns, never the whole profile and never contact details. It must
//!   say in `why` what was assumed or what is not known, and never ask what
//!   it knows or what the employer's own website covers.
//! - **Answer** reads the person's words into proposed changes (1 call) with
//!   the same structure and checks as "Anything else": a quote that is theirs
//!   and a value that says only what they said; what fails is **flagged,
//!   never dropped**. The changes join the pending ones and the portrait
//!   updates at once; nothing is saved until Save.
//!
//! *Skip* marks the gap skipped for this profile version, and *That's enough
//! for now* ends the questions. Both are free. Every model call counts toward
//! the wizard's daily cap and the AI budget; nothing runs in the background.

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    AboutView, LocaleQuery, MAX_EXTRA, Row,
    gaps::{self, Gap, GapKind, Inputs},
    judge::{self, Change, Field, MAX_SENTENCE, RawChanges, Sentence},
    prompt,
};
use crate::{
    features::{
        completion::current,
        grill::{self, prompt::language},
        imports::Ai,
    },
    goals::Goals,
    ports::Researcher,
    schema::{Area, Profile},
    strength,
};

/// Longest question, in characters.
const MAX_QUESTION: usize = 300;
/// Most choices a closed question offers.
const MAX_CHOICES: usize = 6;
/// Longest choice, in characters.
const MAX_CHOICE: usize = 80;

// ---- what is stored -----------------------------------------------------------

/// What the question says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    /// "I assumed": the question confirms or corrects an assumption.
    Assumed,
    /// "I don't know": there is no basis to assume.
    Unknown,
    /// "You told me": it follows up on something they said.
    Told,
}

impl Tag {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "assumed" => Some(Self::Assumed),
            "unknown" => Some(Self::Unknown),
            "told" => Some(Self::Told),
            _ => None,
        }
    }
}

/// The question on screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asked {
    pub gap: Gap,
    pub question: String,
    pub why: String,
    pub tag: Tag,
    pub choices: Vec<String>,
}

/// Where the questions are for this profile version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    /// Gaps answered.
    #[serde(default)]
    pub answered: Vec<String>,
    /// Gaps skipped for this profile version.
    #[serde(default)]
    pub skipped: Vec<String>,
    /// The question on screen.
    #[serde(default)]
    pub current: Option<Asked>,
    /// "That's enough for now".
    #[serde(default)]
    pub enough: bool,
    /// The changes the last answer proposed.
    #[serde(default)]
    pub updated: Vec<String>,
}

impl Progress {
    fn asked(&self) -> usize {
        self.answered.len() + self.skipped.len()
    }
}

// ---- what is shown --------------------------------------------------------------

/// The question to answer now.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuestionView {
    /// The gap's id: what the answer and skip routes name.
    pub id: String,
    pub kind: GapKind,
    /// The position it is about (its place in `work`).
    pub position: Option<u32>,
    pub area: Option<Area>,
    pub question: String,
    /// What was assumed, or what is not known: why it is asked.
    pub why: String,
    pub tag: Tag,
    /// For a closed question, the answers to pick from.
    pub choices: Vec<String>,
    /// The profile's strength if this is answered, 0 to 100 (the same as
    /// `strength` when the answer is not a part of the score).
    pub strength_after: u8,
}

/// The questions about the gaps.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuestionsView {
    /// The question to ask now; none until `POST …/questions/next` made it.
    pub current: Option<QuestionView>,
    /// Questions answered.
    pub answered: u32,
    /// Questions skipped.
    pub skipped: u32,
    /// About how many questions in all: the asked ones and those left.
    pub total: u32,
    /// Gaps left to ask about, the question on screen included.
    pub left: u32,
    /// Nothing more to ask: the gaps ran out, the limit was reached or the
    /// person said "That's enough for now".
    pub done: bool,
    /// The profile's strength with everything told so far, as if saved.
    pub strength: u8,
    /// The changes (by id) the last answer proposed.
    pub updated: Vec<String>,
}

impl QuestionsView {
    fn none() -> Self {
        Self {
            current: None,
            answered: 0,
            skipped: 0,
            total: 0,
            left: 0,
            done: false,
            strength: 0,
            updated: Vec::new(),
        }
    }
}

fn counted(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The skills the person said are not theirs (lower case).
async fn dismissed(db: &sqlx::PgPool, account: Uuid) -> Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT name_key FROM profile.skill_dismissals WHERE account_id = $1")
            .bind(account)
            .fetch_all(db)
            .await?,
    )
}

/// The gaps still to ask about: not answered, not skipped, within the limit
/// that "Is this accurate?" sets.
pub fn remaining(inputs: &Inputs<'_>, progress: &Progress) -> Vec<Gap> {
    let room = gaps::limit(inputs.accuracy).saturating_sub(progress.asked());
    gaps::gaps(inputs)
        .into_iter()
        .filter(|g| !progress.answered.contains(&g.id) && !progress.skipped.contains(&g.id))
        .take(room)
        .collect()
}

fn inputs_of<'a>(
    profile: &'a Profile,
    goals: &'a Goals,
    row: &'a Row,
    dismissed: &'a [String],
) -> Inputs<'a> {
    Inputs {
        profile,
        goals,
        portrait: &row.portrait.0,
        choices: &row.choices.0,
        changes: &row.changes.0,
        accuracy: row.accuracy.as_deref().and_then(super::Accuracy::parse),
        dismissed,
    }
}

/// The questions as they stand: never calls the model.
pub(super) async fn state(
    state: &AppState,
    account: Uuid,
    seen: Option<&crate::features::completion::Current>,
    row: &Row,
    built: bool,
) -> Result<QuestionsView> {
    let Some(seen) = seen.filter(|_| built) else {
        return Ok(QuestionsView::none());
    };
    let now = state.clock.now();
    let skills_out = dismissed(&state.db, account).await?;
    let inputs = inputs_of(&seen.profile, &seen.goals, row, &skills_out);
    let left = remaining(&inputs, &row.questions.0);

    // The strength as if everything told so far were saved.
    let mut profile = seen.profile.clone();
    let mut goals = seen.goals.clone();
    super::apply(row, &mut profile, &mut goals, now);
    let strength_now = strength::compute(&profile, &goals).score;

    let progress = &row.questions.0;
    let current = progress
        .current
        .as_ref()
        .filter(|asked| left.iter().any(|g| g.id == asked.gap.id))
        .map(|asked| QuestionView {
            id: asked.gap.id.clone(),
            kind: asked.gap.kind,
            position: asked.gap.position,
            area: asked.gap.area,
            question: asked.question.clone(),
            why: asked.why.clone(),
            tag: asked.tag,
            choices: asked.choices.clone(),
            strength_after: strength_now
                .saturating_add(gaps::gain(&asked.gap, &profile, &goals, now))
                .min(100),
        });
    Ok(QuestionsView {
        current,
        answered: counted(progress.answered.len()),
        skipped: counted(progress.skipped.len()),
        total: counted(progress.asked() + left.len()),
        left: counted(left.len()),
        done: progress.enough || left.is_empty(),
        strength: strength_now,
        updated: progress.updated.clone(),
    })
}

// ---- the model's part -----------------------------------------------------------

/// A question as the model worded it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawQuestion {
    pub question: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub choices: Vec<String>,
}

/// The question as asked: tidied, with a reason always. A reason the model
/// left out is the gap's own basis, never nothing; an assumption is always
/// tagged as one, and a claim from "Anything else" as told.
pub fn judge_question(raw: RawQuestion, gap: &Gap) -> Option<Asked> {
    let question = judge::cut(&raw.question, MAX_QUESTION);
    if question.is_empty() {
        return None;
    }
    let why = Some(judge::cut(&raw.why, MAX_SENTENCE))
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| judge::cut(&gap.basis, MAX_SENTENCE));
    let tag = match gap.kind {
        GapKind::Assumed => Tag::Assumed,
        GapKind::Claim => Tag::Told,
        _ => Tag::parse(&raw.tag).unwrap_or(Tag::Unknown),
    };
    let mut choices: Vec<String> = Vec::new();
    for choice in raw.choices {
        let choice = judge::cut(&choice, MAX_CHOICE);
        if !choice.is_empty()
            && choices.len() < MAX_CHOICES
            && !choices.iter().any(|c| {
                applicamp_kernel::claims::squash(c) == applicamp_kernel::claims::squash(&choice)
            })
        {
            choices.push(choice);
        }
    }
    Some(Asked {
        gap: gap.clone(),
        question,
        why,
        tag,
        choices,
    })
}

/// Fit the changes an answer proposed to the gap it answers: a fact belongs
/// to the position asked about, in the area asked about.
pub fn fit_to_gap(changes: &mut [Change], gap: &Gap, role_keys: &[String]) {
    for change in changes {
        change.origin = Some(gap.id.clone());
        if change.field != Field::RoleFact {
            continue;
        }
        if let Some(position) = gap.position {
            change.position = Some(position);
            change.role_key = usize::try_from(position)
                .ok()
                .and_then(|p| role_keys.get(p).cloned());
        }
        // An assumption's topic is the app's guess; the area of any other gap
        // is what was asked.
        if let Some(area) = gap.area.filter(|_| gap.kind != GapKind::Assumed) {
            change.area = area;
        }
    }
}

/// Add `new` to `pending`: ids of their own, a single-valued field replaced
/// by the newer answer, and what is already there left alone. The ids added.
pub fn merge(pending: &mut Vec<Change>, new: Vec<Change>) -> Vec<String> {
    let mut next = pending
        .iter()
        .filter_map(|c| c.id.strip_prefix('q')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let mut added = Vec::new();
    for mut change in new {
        if change.field.single() {
            pending.retain(|c| c.field != change.field);
        } else if pending
            .iter()
            .any(|c| c.field == change.field && judge::same(change.field, &c.value, &change.value))
        {
            continue;
        }
        change.id = format!("q{next}");
        next += 1;
        added.push(change.id.clone());
        pending.push(change);
    }
    added
}

// ---- requests ---------------------------------------------------------------------

/// An answer to a question.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerRequest {
    /// In their own words, up to 4000 characters; may be empty when a choice
    /// is picked.
    #[serde(default)]
    pub text: String,
    /// The choices picked, from the question's own.
    #[serde(default)]
    pub choices: Vec<String>,
}

fn invalid(code: &'static str, detail: impl Into<String>) -> Error {
    Error::BadRequest(code, detail.into())
}

fn portrait_needed() -> Error {
    Error::Conflict(
        "portrait_needed",
        "Start with the portrait: the questions come after it.".to_owned(),
    )
}

/// The wizard's row for the profile as it is now; the portrait must have been
/// made for it.
async fn built_row(
    state: &AppState,
    account: Uuid,
    version: i32,
    now: DateTime<Utc>,
) -> Result<Row> {
    let row = super::row_of(&state.db, account, now)
        .await?
        .ok_or_else(portrait_needed)?;
    if row.profile_version == Some(version) {
        Ok(row)
    } else {
        Err(portrait_needed())
    }
}

// ---- endpoints --------------------------------------------------------------------

/// Ask the next question.
///
/// The question for the top gap, or the one already on screen (no call).
/// **One small model call** words it: the model sees only that gap and its
/// context, and says why it asks. When there is nothing more to ask, no call
/// is made and the state says `done`.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/questions/next",
    tag = "profile",
    operation_id = "profile.about_question_next",
    security(("session" = [])),
    params(LocaleQuery),
    responses(
        (status = 200, description = "The wizard's state, with the question.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`; `portrait_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`about_daily_cap`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`; `ai_daily_limit_reached`; \
            `ai_budget_reached`; `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn next(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
    Extension(research): Extension<Researcher>,
    Query(query): Query<LocaleQuery>,
) -> Result<Json<AboutView>> {
    let account = principal.account_id;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(super::needs_profile)?;
    let now = state.clock.now();
    let row = built_row(&state, account, seen.version, now).await?;
    let skills_out = dismissed(&state.db, account).await?;
    let left = remaining(
        &inputs_of(&seen.profile, &seen.goals, &row, &skills_out),
        &row.questions.0,
    );
    let on_screen = row
        .questions
        .0
        .current
        .as_ref()
        .is_some_and(|asked| left.iter().any(|g| g.id == asked.gap.id));
    let Some(top) = left
        .first()
        .filter(|_| !row.questions.0.enough && !on_screen)
    else {
        return super::respond(&state, &research, account).await;
    };

    let gateway = super::gateway(&ai)?;
    if super::over_cap(super::calls_today(&state.db, account, now).await?, 1) {
        return Err(super::cap_reached());
    }
    let assumption: Option<&Sentence> = top
        .id
        .strip_prefix("assumed:")
        .and_then(|id| row.portrait.0.iter().find(|s| s.id == id));
    let message = prompt::question_message(
        &seen.profile,
        top,
        assumption,
        language(query.locale.as_deref()).as_deref(),
        &now.date_naive().to_string(),
    );
    let raw: RawQuestion = super::ask(
        gateway,
        &state.db,
        account,
        now,
        "profile_about_question",
        &prompt::question_schema(),
        prompt::QUESTION_INSTRUCTIONS,
        &message,
    )
    .await?;
    super::record_call(&state.db, account, "question", now).await?;
    let asked = judge_question(raw, top).ok_or(Error::Unavailable(applicamp_ai::AI_FAILED))?;
    // Ids and counts only: never the question or the profile.
    tracing::info!(
        prompt = prompt::PROMPT_VERSION,
        kind = ?asked.gap.kind,
        choices = asked.choices.len(),
        "an about-you question worded"
    );

    let mut tx = state.db.begin().await?;
    let mut row = super::lock(&mut tx, account, now)
        .await?
        .ok_or_else(portrait_needed)?;
    row.questions.0.current = Some(asked);
    super::put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    super::respond(&state, &research, account).await
}

/// Answer the question on screen.
///
/// **One small model call** reads the answer (and the choices picked) into
/// proposed changes, with the same structure and checks as "Anything else":
/// each quotes their words, and a claim their words do not say is a flag,
/// shown and never dropped. The changes join the pending ones, and the
/// portrait gains their sentences as new. Nothing is saved yet.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/questions/{id}/answer",
    tag = "profile",
    operation_id = "profile.about_question_answer",
    security(("session" = [])),
    params(
        ("id" = String, Path, description = "The question on screen."),
        LocaleQuery,
    ),
    request_body = AnswerRequest,
    responses(
        (status = 200, description = "The wizard's state, with the changes.", body = AboutView),
        (status = 400, description = "`invalid_text`; `invalid_choice`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: that is not the question on screen",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`profile_needed`; `portrait_needed`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`about_daily_cap`", body = Problem,
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
    Extension(research): Extension<Researcher>,
    Path(id): Path<String>,
    Query(query): Query<LocaleQuery>,
    body: std::result::Result<Json<AnswerRequest>, JsonRejection>,
) -> Result<Json<AboutView>> {
    let Json(request) = body.map_err(|rejection| invalid("invalid_text", rejection.body_text()))?;
    let account = principal.account_id;
    let seen = current(&state.db, account)
        .await?
        .ok_or_else(super::needs_profile)?;
    let now = state.clock.now();
    let row = built_row(&state, account, seen.version, now).await?;
    let asked = row
        .questions
        .0
        .current
        .clone()
        .filter(|asked| asked.gap.id == id)
        .ok_or(Error::NotFound)?;

    // Their words: what they wrote and what they picked (from the question's
    // own choices).
    let mut words = request.text.trim().to_owned();
    for choice in &request.choices {
        if !asked.choices.contains(choice) {
            return Err(invalid("invalid_choice", "Pick one of the choices shown."));
        }
        if !words.is_empty() {
            words.push('\n');
        }
        words.push_str(choice);
    }
    if words.is_empty() || words.chars().count() > MAX_EXTRA {
        return Err(invalid(
            "invalid_text",
            format!("Write between 1 and {MAX_EXTRA} characters."),
        ));
    }

    let gateway = super::gateway(&ai)?;
    if super::over_cap(super::calls_today(&state.db, account, now).await?, 1) {
        return Err(super::cap_reached());
    }
    let message = prompt::answer_message(
        &seen.profile,
        &seen.goals,
        &asked.gap,
        &asked.question,
        &words,
        language(query.locale.as_deref()).as_deref(),
    );
    let raw: RawChanges = super::ask(
        gateway,
        &state.db,
        account,
        now,
        "profile_about_answer",
        &prompt::extra_schema(),
        prompt::ANSWER_INSTRUCTIONS,
        &message,
    )
    .await?;
    super::record_call(&state.db, account, "answer", now).await?;
    let role_keys: Vec<String> = seen.profile.work.iter().map(grill::role_key).collect();
    let mut changes = judge::judge_changes(raw, &words, &role_keys);
    fit_to_gap(&mut changes, &asked.gap, &role_keys);
    tracing::info!(
        prompt = prompt::PROMPT_VERSION,
        kind = ?asked.gap.kind,
        changes = changes.len(),
        flagged = changes.iter().filter(|c| !c.flags.is_empty()).count(),
        "an about-you answer read"
    );

    let mut tx = state.db.begin().await?;
    let mut row = super::lock(&mut tx, account, now)
        .await?
        .ok_or_else(portrait_needed)?;
    // Another tab may have answered or skipped it meanwhile.
    if row.questions.0.current.as_ref().map(|c| &c.gap.id) != Some(&id) {
        return Err(Error::NotFound);
    }
    let added = merge(&mut row.changes.0, changes);
    row.questions.0.answered.push(id);
    row.questions.0.current = None;
    row.questions.0.updated = added;
    super::put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    super::respond(&state, &research, account).await
}

/// Skip the question on screen, for this profile version. No model call.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/questions/{id}/skip",
    tag = "profile",
    operation_id = "profile.about_question_skip",
    security(("session" = [])),
    params(("id" = String, Path, description = "The question on screen.")),
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: that is not the question on screen",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn skip(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
    Path(id): Path<String>,
) -> Result<Json<AboutView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = super::lock(&mut tx, account, now)
        .await?
        .ok_or(Error::NotFound)?;
    if row.questions.0.current.as_ref().map(|c| &c.gap.id) != Some(&id) {
        return Err(Error::NotFound);
    }
    row.questions.0.skipped.push(id);
    row.questions.0.current = None;
    row.questions.0.updated = Vec::new();
    super::put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    super::respond(&state, &research, account).await
}

/// "That's enough for now": no more questions; on to the review. No model
/// call.
#[utoipa::path(
    post,
    path = "/api/v1/profile/about/questions/enough",
    tag = "profile",
    operation_id = "profile.about_questions_enough",
    security(("session" = [])),
    responses(
        (status = 200, description = "The wizard's state.", body = AboutView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn enough(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(research): Extension<Researcher>,
) -> Result<Json<AboutView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let mut row = super::lock(&mut tx, account, now)
        .await?
        .unwrap_or_else(Row::empty);
    row.questions.0.enough = true;
    row.questions.0.current = None;
    super::put(&mut tx, account, &row, now).await?;
    tx.commit().await?;
    super::respond(&state, &research, account).await
}
