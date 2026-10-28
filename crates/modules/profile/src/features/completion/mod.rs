//! Profile completion: how strong the profile is, the questions the AI asks
//! to fill its gaps (one review per profile and goals version), drafts made
//! from the person's answers, and the skills their positions mention.
//!
//! Nothing here writes to the profile unless the person keeps a draft or
//! adds a skill, and every question can be skipped.

mod check;
pub mod draft;
pub mod review;
pub mod skills;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn, jobs};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    events::ProfileUpdated,
    features::imports::Ai,
    goals::Goals,
    schema::{Education, Link, Profile},
    strength::{self, StrengthKey, StrengthState as PartState},
};

/// What a question is about, and where its answer goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum QuestionSection {
    /// A position: its answer becomes achievements of that position.
    #[serde(rename = "work")]
    Work,
    /// The headline.
    #[serde(rename = "basics.label")]
    BasicsLabel,
    /// The summary.
    #[serde(rename = "basics.summary")]
    BasicsSummary,
    /// A school.
    #[serde(rename = "education")]
    Education,
    /// A link: GitHub, a portfolio.
    #[serde(rename = "links")]
    Links,
}

impl QuestionSection {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "work" => Some(Self::Work),
            "basics.label" => Some(Self::BasicsLabel),
            "basics.summary" => Some(Self::BasicsSummary),
            "education" => Some(Self::Education),
            "links" => Some(Self::Links),
            _ => None,
        }
    }
}

/// A question as the review stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredQuestion {
    pub(crate) id: String,
    pub(crate) section: QuestionSection,
    pub(crate) role_ref: Option<u32>,
    pub(crate) question: String,
    pub(crate) why: String,
}

/// A skill the positions mention, as the review stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredSkill {
    pub(crate) name: String,
    pub(crate) roles: Vec<u32>,
}

/// The profile and goals a review is made from.
pub(crate) struct Current {
    pub(crate) profile: Profile,
    pub(crate) version: i32,
    pub(crate) goals: Goals,
    pub(crate) goals_version: i32,
}

/// The profile and goals of `account`; none until there is a profile.
pub(crate) async fn current(db: &PgPool, account: Uuid) -> Result<Option<Current>> {
    let row: Option<(sqlx::types::Json<Profile>, i32)> =
        sqlx::query_as("SELECT data, version FROM profile.profiles WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    let Some((profile, version)) = row else {
        return Ok(None);
    };
    let goals: Option<(sqlx::types::Json<Goals>, i32)> =
        sqlx::query_as("SELECT answers, version FROM profile.goals WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    let (goals, goals_version) = goals.map_or((Goals::default(), 0), |(g, v)| (g.0, v));
    Ok(Some(Current {
        profile: profile.0,
        version,
        goals,
        goals_version,
    }))
}

/// The score `section`'s question would add if the person answered it: the
/// strength with a result added to the position, or the section filled.
pub(crate) fn gain_of(
    section: QuestionSection,
    role_ref: Option<u32>,
    profile: &Profile,
    goals: &Goals,
) -> u8 {
    let before = strength::compute(profile, goals).score;
    let mut after = profile.clone();
    match section {
        QuestionSection::Work => {
            if let Some(work) = role_ref
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| after.work.get_mut(i))
            {
                work.highlights.push("1".to_owned());
            }
        }
        QuestionSection::BasicsLabel => after.basics.label = Some("x".to_owned()),
        QuestionSection::BasicsSummary => after.basics.summary = Some("x".to_owned()),
        QuestionSection::Education => after.education.push(Education {
            institution: "x".to_owned(),
            ..Education::default()
        }),
        QuestionSection::Links => after.basics.profiles.push(Link {
            network: None,
            url: "x".to_owned(),
        }),
    }
    strength::compute(&after, goals)
        .score
        .saturating_sub(before)
}

/// The review of one profile and goals version, as stored.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ReviewRow {
    pub(crate) profile_version: i32,
    pub(crate) goals_version: i32,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) questions: sqlx::types::Json<Vec<StoredQuestion>>,
    pub(crate) skills: sqlx::types::Json<Vec<StoredSkill>>,
    pub(crate) resolved: sqlx::types::Json<BTreeMap<String, String>>,
}

impl ReviewRow {
    /// The questions not yet kept or skipped, the most useful first, each
    /// with the score it would add now.
    pub(crate) fn open(&self, profile: &Profile, goals: &Goals) -> Vec<(StoredQuestion, u8)> {
        let mut open: Vec<(StoredQuestion, u8)> = self
            .questions
            .0
            .iter()
            .filter(|q| !self.resolved.0.contains_key(&q.id))
            .map(|q| (q.clone(), gain_of(q.section, q.role_ref, profile, goals)))
            .collect();
        open.sort_by_key(|(_, gain)| std::cmp::Reverse(*gain));
        open
    }
}

/// Where the review of the current versions is.
pub(crate) enum Review {
    /// There is no profile yet.
    Empty,
    /// No model is configured, and none was made.
    Unavailable(&'static str),
    /// The model is reading.
    Running,
    /// The model could not be asked or answered (a problem code).
    Failed(String),
    /// Made.
    Ready(Box<ReviewRow>),
}

/// The stored review of `account` if it is for these versions.
async fn stored(db: &PgPool, account: Uuid, current: &Current) -> Result<Option<ReviewRow>> {
    let row: Option<ReviewRow> = sqlx::query_as(
        "SELECT profile_version, goals_version, status, error, questions, skills, resolved
           FROM profile.gap_reviews WHERE account_id = $1",
    )
    .bind(account)
    .fetch_optional(db)
    .await?;
    Ok(row.filter(|r| {
        r.profile_version == current.version && r.goals_version == current.goals_version
    }))
}

/// The review for the current profile and goals: the stored one, or a new
/// one started in the background (one model call per version). `retry`
/// starts a failed one again; nothing else ever repeats a call.
pub(crate) async fn ensure(
    state: &AppState,
    ai: &Ai,
    account: Uuid,
    retry: bool,
) -> Result<(Option<Current>, Review)> {
    let Some(current) = current(&state.db, account).await? else {
        return Ok((None, Review::Empty));
    };
    if current.profile.is_empty() {
        return Ok((Some(current), Review::Empty));
    }
    if let Some(row) = stored(&state.db, account, &current).await? {
        match row.status.as_str() {
            "ready" => return Ok((Some(current), Review::Ready(Box::new(row)))),
            "running" => return Ok((Some(current), Review::Running)),
            _ if !retry => {
                let code = row
                    .error
                    .unwrap_or_else(|| applicamp_ai::AI_FAILED.to_owned());
                return Ok((Some(current), Review::Failed(code)));
            }
            _ => {}
        }
    }
    if !ai.can_structure() {
        return Ok((
            Some(current),
            Review::Unavailable(applicamp_ai::AI_UNAVAILABLE),
        ));
    }
    let mut tx = state.db.begin().await?;
    let started = sqlx::query(
        "INSERT INTO profile.gap_reviews (account_id, profile_version, goals_version, status, created_at)
         VALUES ($1, $2, $3, 'running', $4)
         ON CONFLICT (account_id) DO UPDATE
            SET profile_version = excluded.profile_version, goals_version = excluded.goals_version,
                status = 'running', error = NULL, questions = '[]', skills = '[]',
                resolved = '{}', created_at = excluded.created_at
          WHERE gap_reviews.profile_version <> excluded.profile_version
             OR gap_reviews.goals_version <> excluded.goals_version
             OR (gap_reviews.status = 'failed' AND $5)",
    )
    .bind(account)
    .bind(current.version)
    .bind(current.goals_version)
    .bind(state.clock.now())
    .bind(retry)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if started == 1 {
        jobs::enqueue(
            &mut tx,
            &review::GapReview {
                account,
                profile_version: current.version,
                goals_version: current.goals_version,
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok((Some(current), Review::Running))
}

/// How many questions of the current review are still open; none when there
/// is no review for the current versions.
pub(crate) async fn questions_left(
    db: &PgPool,
    account: Uuid,
    current: &Current,
) -> Result<Option<u32>> {
    Ok(stored(db, account, current)
        .await?
        .filter(|row| row.status == "ready")
        .map(|row| {
            u32::try_from(row.open(&current.profile, &current.goals).len()).unwrap_or(u32::MAX)
        }))
}

/// Write `profile` as the account's profile, inside `tx`: the version goes up
/// by one (a trigger). `source` replaces where it came from when given.
/// Publishes `profile.updated`. The new version.
pub(crate) async fn write_profile(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    account: Uuid,
    profile: &Profile,
    source: Option<&str>,
) -> Result<i32> {
    let version: i32 = sqlx::query_scalar(
        "UPDATE profile.profiles
            SET data = $2, source = COALESCE($3, source), updated_at = $4
          WHERE account_id = $1 RETURNING version",
    )
    .bind(account)
    .bind(sqlx::types::Json(profile))
    .bind(source)
    .bind(state.clock.now())
    .fetch_one(&mut *tx)
    .await?;
    state
        .events
        .publish(tx, &ProfileUpdated { account })
        .await?;
    Ok(version)
}

/// The review made for `old_version` now follows the profile to
/// `new_version`: the person's own kept answer or added skill did not make
/// it stale. `resolved` marks a question as `kept` or `skipped`.
pub(crate) async fn carry_forward(
    tx: &mut sqlx::PgConnection,
    account: Uuid,
    old_version: i32,
    new_version: i32,
    resolved: Option<(&str, &str)>,
) -> Result<()> {
    let marks = resolved.map_or_else(
        || serde_json::json!({}),
        |(id, outcome)| serde_json::json!({ id: outcome }),
    );
    sqlx::query(
        "UPDATE profile.gap_reviews
            SET profile_version = $3, resolved = resolved || $4
          WHERE account_id = $1 AND profile_version = $2 AND status = 'ready'",
    )
    .bind(account)
    .bind(old_version)
    .bind(new_version)
    .bind(marks)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

// ---- strength -------------------------------------------------------------

/// One part of the score.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StrengthItem {
    /// Which part.
    pub key: StrengthKey,
    /// Where it is.
    pub state: PartState,
    /// Points earned.
    pub earned: u32,
    /// Points it is worth.
    pub weight: u32,
    /// What there is: skills listed, recent roles with results, positions…
    pub have: u32,
    /// What there should be for the full weight; 0 when it is a yes or no.
    pub of: u32,
    /// What to do about it, when it is not full: `question:<id>` (an open
    /// question about it), `skills`, `import`, `goals`, or
    /// `edit:<section>` (`basics`, `work`, `education`, `languages`).
    pub action: Option<String>,
}

/// How strong the profile is.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StrengthView {
    /// 0 to 100.
    pub score: u8,
    /// At least 80.
    pub strong: bool,
    /// The parts, in checklist order.
    pub items: Vec<StrengthItem>,
    /// Open questions of the review for the current profile; none when
    /// there is no review (yet).
    pub questions_left: Option<u32>,
    /// Skills the positions mention that the profile does not list; none
    /// when there is no review (yet).
    pub suggested_skills: Option<u32>,
}

/// The action for a part that is not full.
fn action_for(key: StrengthKey, open: &[(StoredQuestion, u8)]) -> String {
    let asking = |wanted: QuestionSection| {
        open.iter()
            .find(|(q, _)| q.section == wanted)
            .map(|(q, _)| format!("question:{}", q.id))
    };
    match key {
        StrengthKey::Headline => {
            asking(QuestionSection::BasicsLabel).unwrap_or_else(|| "edit:basics".to_owned())
        }
        StrengthKey::Summary => {
            asking(QuestionSection::BasicsSummary).unwrap_or_else(|| "edit:basics".to_owned())
        }
        StrengthKey::Achievements => {
            asking(QuestionSection::Work).unwrap_or_else(|| "edit:work".to_owned())
        }
        StrengthKey::Skills => "skills".to_owned(),
        StrengthKey::Education => {
            asking(QuestionSection::Education).unwrap_or_else(|| "edit:education".to_owned())
        }
        StrengthKey::Languages => "edit:languages".to_owned(),
        StrengthKey::Links => {
            asking(QuestionSection::Links).unwrap_or_else(|| "edit:basics".to_owned())
        }
        StrengthKey::Positions => "import".to_owned(),
        StrengthKey::Goals => "goals".to_owned(),
    }
}

/// The strength of the signed-in person's profile.
///
/// Computed from the profile and goals, never by a model. Each part that is
/// not full says what to do about it. Reading it never starts a review.
#[utoipa::path(
    get,
    path = "/api/v1/profile/strength",
    tag = "profile",
    operation_id = "profile.strength",
    security(("session" = [])),
    responses(
        (status = 200, description = "The strength.", body = StrengthView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get_strength(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<StrengthView>> {
    let current = current(&state.db, principal.account_id).await?;
    let Some(current) = current else {
        let strength = strength::compute(&Profile::default(), &Goals::default());
        return Ok(Json(view_of(&strength, &[], None, None)));
    };
    let strength = strength::compute(&current.profile, &current.goals);
    let row = stored(&state.db, principal.account_id, &current)
        .await?
        .filter(|row| row.status == "ready");
    let open = row
        .as_ref()
        .map(|row| row.open(&current.profile, &current.goals));
    let suggested = match &row {
        Some(row) => Some(
            skills::visible(&state.db, principal.account_id, &current.profile, row)
                .await?
                .len(),
        ),
        None => None,
    };
    Ok(Json(view_of(
        &strength,
        open.as_deref().unwrap_or(&[]),
        open.as_ref()
            .map(|o| u32::try_from(o.len()).unwrap_or(u32::MAX)),
        suggested.map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
    )))
}

fn view_of(
    strength: &strength::Strength,
    open: &[(StoredQuestion, u8)],
    questions_left: Option<u32>,
    suggested_skills: Option<u32>,
) -> StrengthView {
    StrengthView {
        score: strength.score,
        strong: strength.is_strong(),
        items: strength
            .items
            .iter()
            .map(|item| StrengthItem {
                key: item.key,
                state: item.state,
                earned: item.earned,
                weight: item.key.weight(),
                have: item.have,
                of: item.of,
                action: (item.state != PartState::Ok).then(|| action_for(item.key, open)),
            })
            .collect(),
        questions_left,
        suggested_skills,
    }
}

// ---- questions ------------------------------------------------------------

/// Where the review is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// The model is reading the profile; ask again in a moment.
    Running,
    /// Made.
    Ready,
    /// No model is configured, or there is no profile to read.
    Unavailable,
    /// It could not be made (`reason` says why); ask again to retry.
    Failed,
}

/// The position a question is about.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleContext {
    /// Its title.
    pub position: Option<String>,
    /// Its company.
    pub company: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    pub start_date: Option<String>,
    /// As `start_date`; none while it is their current position.
    pub end_date: Option<String>,
}

/// A question about a gap in the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Question {
    /// The question's id.
    pub id: String,
    /// What it is about.
    pub section: QuestionSection,
    /// The position it is about, for `work`.
    pub context: Option<RoleContext>,
    /// The question, at most 160 characters.
    pub question: String,
    /// Why it matters, at most 140 characters.
    pub why: String,
    /// Points the profile strength would gain if it were answered.
    pub expected_gain: u8,
}

/// The questions that would fill the profile's gaps.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuestionsView {
    /// Where the review is.
    pub status: ReviewStatus,
    /// Why it is unavailable or failed: `ai_unavailable`,
    /// `ai_budget_reached`, `ai_daily_limit_reached`, `ai_failed`; none when
    /// there is simply no profile.
    pub reason: Option<String>,
    /// The profile strength now.
    pub score: u8,
    /// Questions in the review.
    pub total: u32,
    /// Questions kept or skipped so far.
    pub answered: u32,
    /// The questions still open, the most useful first.
    pub questions: Vec<Question>,
}

fn questions_view(
    status: ReviewStatus,
    reason: Option<String>,
    current: Option<&Current>,
    row: Option<&ReviewRow>,
) -> QuestionsView {
    let score = current.map_or(0, |c| strength::compute(&c.profile, &c.goals).score);
    let (Some(current), Some(row)) = (current, row) else {
        return QuestionsView {
            status,
            reason,
            score,
            total: 0,
            answered: 0,
            questions: Vec::new(),
        };
    };
    let total = u32::try_from(row.questions.0.len()).unwrap_or(u32::MAX);
    let open = row.open(&current.profile, &current.goals);
    let answered = total.saturating_sub(u32::try_from(open.len()).unwrap_or(u32::MAX));
    QuestionsView {
        status,
        reason,
        score,
        total,
        answered,
        questions: open
            .into_iter()
            .map(|(q, gain)| Question {
                context: q
                    .role_ref
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| current.profile.work.get(i))
                    .map(|work| RoleContext {
                        position: work.position.clone(),
                        company: work.name.clone(),
                        start_date: work.start_date.clone(),
                        end_date: work.end_date.clone(),
                    }),
                id: q.id,
                section: q.section,
                question: q.question,
                why: q.why,
                expected_gain: gain,
            })
            .collect(),
    }
}

async fn questions_of(
    state: &AppState,
    ai: &Ai,
    account: Uuid,
    retry: bool,
) -> Result<(StatusCode, Json<QuestionsView>)> {
    let (current, review) = ensure(state, ai, account, retry).await?;
    let (code, view) = match review {
        Review::Empty => (
            StatusCode::OK,
            questions_view(ReviewStatus::Unavailable, None, current.as_ref(), None),
        ),
        Review::Unavailable(reason) => (
            StatusCode::OK,
            questions_view(
                ReviewStatus::Unavailable,
                Some(reason.to_owned()),
                current.as_ref(),
                None,
            ),
        ),
        Review::Running => (
            StatusCode::ACCEPTED,
            questions_view(ReviewStatus::Running, None, current.as_ref(), None),
        ),
        Review::Failed(reason) => (
            StatusCode::OK,
            questions_view(ReviewStatus::Failed, Some(reason), current.as_ref(), None),
        ),
        Review::Ready(row) => (
            StatusCode::OK,
            questions_view(ReviewStatus::Ready, None, current.as_ref(), Some(&row)),
        ),
    };
    Ok((code, Json(view)))
}

/// The questions that would make the profile stronger.
///
/// The first request for a profile and goals version starts the AI review in
/// the background (one model call) and answers 202 `running`; ask again in a
/// moment. The review is kept until the profile or goals change by any other
/// way than the person's own kept answers. Without a model, or over a
/// spending limit, the status says so (`unavailable`, `failed`) and the
/// checklist from `GET /api/v1/profile/strength` still works.
#[utoipa::path(
    get,
    path = "/api/v1/profile/questions",
    tag = "profile",
    operation_id = "profile.questions",
    security(("session" = [])),
    responses(
        (status = 200, description = "The questions (or why there are none).",
            body = QuestionsView),
        (status = 202, description = "The review is running.", body = QuestionsView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get_questions(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
) -> Result<(StatusCode, Json<QuestionsView>)> {
    questions_of(&state, &ai, principal.account_id, false).await
}

/// Try the review again after it failed.
///
/// Only a failed review is started again, and the spending limits still
/// apply: over one, it fails again at once without calling the model.
#[utoipa::path(
    post,
    path = "/api/v1/profile/questions/review",
    tag = "profile",
    operation_id = "profile.review_again",
    security(("session" = [])),
    responses(
        (status = 200, description = "The questions (or why there are none).",
            body = QuestionsView),
        (status = 202, description = "The review is running.", body = QuestionsView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn review_again(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(ai): Extension<Ai>,
) -> Result<(StatusCode, Json<QuestionsView>)> {
    questions_of(&state, &ai, principal.account_id, true).await
}

/// Skip a question.
///
/// It is not asked again for this version of the profile.
#[utoipa::path(
    post,
    path = "/api/v1/profile/questions/{id}/skip",
    tag = "profile",
    operation_id = "profile.skip_question",
    security(("session" = [])),
    params(("id" = String, Path, description = "The question.")),
    responses(
        (status = 204, description = "Skipped."),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn skip_question(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let account = principal.account_id;
    let current = current(&state.db, account).await?.ok_or(Error::NotFound)?;
    let row = stored(&state.db, account, &current)
        .await?
        .filter(|row| row.status == "ready")
        .ok_or(Error::NotFound)?;
    if !row.questions.0.iter().any(|q| q.id == id) {
        return Err(Error::NotFound);
    }
    let mut tx = state.db.begin().await?;
    carry_forward(
        &mut tx,
        account,
        current.version,
        current.version,
        Some((&id, "skipped")),
    )
    .await?;
    sqlx::query(
        "UPDATE profile.drafts SET status = 'discarded'
          WHERE account_id = $1 AND question_id = $2 AND status = 'ready'",
    )
    .bind(account)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
