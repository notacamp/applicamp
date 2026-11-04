//! Complete your profile (#72): the facts the app already knows about a person
//! that their profile does not hold yet, proposed to keep or drop.
//!
//! [`propose`] finds them in plain code, with no model: what the person told
//! and confirmed in Tell me about you, what their own website says (confirmed
//! the same way), and what a CV read again says that a position lacks. They
//! are stored as proposals (`profile.fact_proposals`), never twice and never
//! one the profile holds; **nothing reaches the profile until the person keeps
//! it**. Keep writes the fact on its position (`work[].facts`, with its quote,
//! as a fact the person told or an assumption they confirmed), adds the skill,
//! or adds to the background: one new profile version per call (Keep all is
//! one). Drop marks it. Either can be undone for a day.
//!
//! Proposals follow what the person does in Tell me about you: one they take
//! back there (an assumption switched to wrong, an answer changed, a change
//! edited or removed, a new portrait) is closed while it waits; a fact they
//! dropped here is never written by Save. They are made by a job
//! (`profile.propose_facts`, ids only) queued whenever the wizard's state
//! changes and when an import is applied; at Save, Tell me about you proposes
//! what it did not save itself in the same transaction, before its pending
//! state is deleted. Once per instance, `profile.backfill_facts` does it for
//! every account that existed before (a marker keeps it from running twice).

pub(crate) mod propose;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use applicamp_kernel::{
    AppState, Error, Job, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row as _};
use utoipa::ToSchema;
use uuid::Uuid;

use self::propose::{
    AboutKnown, Candidate, Key, Origin, already_held, candidates, key_of, norm, position_by_key,
};
use crate::{
    features::{
        about::{
            Choice,
            judge::{Change, Sentence},
        },
        completion,
    },
    schema::{Area, FactStatus, Profile, ProfileFact, Skill},
};

/// How long a decision can be undone.
pub const UNDO_LIFETIME: TimeDelta = TimeDelta::hours(24);
/// How long after an import was applied the page says it read the CV again.
const CV_READ_SHOWN: TimeDelta = TimeDelta::hours(24);

// ---- what is stored and shown ------------------------------------------------

/// Where a proposed fact goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactTarget {
    /// A position: a fact on it.
    Position,
    /// The skills: one more skill.
    Skills,
    /// The background: where the person comes from.
    Background,
}

impl FactTarget {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Skills => "skills",
            Self::Background => "background",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "position" => Some(Self::Position),
            "skills" => Some(Self::Skills),
            "background" => Some(Self::Background),
            _ => None,
        }
    }
}

/// Where a proposed fact comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactSource {
    /// The person wrote it: under "Anything else", or answering a question.
    YouSaid,
    /// An assumption the person confirmed, or a sentence of the portrait they
    /// said is accurate.
    YouConfirmed,
    /// Their own website says it (an assumption they confirmed).
    YourWebsite,
    /// A CV or a LinkedIn profile they imported again.
    YourCv,
}

impl FactSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::YouSaid => "you_said",
            Self::YouConfirmed => "you_confirmed",
            Self::YourWebsite => "your_website",
            Self::YourCv => "your_cv",
        }
    }

    /// How a fact from it is stored once kept: the person's own words, or an
    /// assumption they confirmed.
    pub(crate) const fn status(self) -> FactStatus {
        match self {
            Self::YouSaid | Self::YourCv => FactStatus::Told,
            Self::YouConfirmed | Self::YourWebsite => FactStatus::ConfirmedAssumption,
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "you_said" => Some(Self::YouSaid),
            "you_confirmed" => Some(Self::YouConfirmed),
            "your_website" => Some(Self::YourWebsite),
            "your_cv" => Some(Self::YourCv),
            _ => None,
        }
    }
}

/// Where a proposal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactProposalState {
    /// Waiting for Keep or Drop.
    Open,
    /// Kept: it is on the profile.
    Kept,
    /// Dropped.
    Dropped,
}

impl FactProposalState {
    fn parse(raw: &str) -> Self {
        match raw {
            "kept" => Self::Kept,
            "dropped" => Self::Dropped,
            _ => Self::Open,
        }
    }
}

/// A fact proposed for the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FactProposalView {
    pub id: Uuid,
    /// The fact (for a skill, its name).
    pub text: String,
    /// The words it comes from: the person's, the confirmed assumption, the
    /// website's or the CV's.
    pub quote: String,
    pub source: FactSource,
    /// For a confirmed assumption: what it was based on (its reason, or the
    /// page it was read on).
    pub basis: Option<String>,
    /// What it is about.
    pub area: Area,
    pub state: FactProposalState,
    /// An assumption the person marked *That's right* in Tell me about you
    /// (not one left unanswered, nor a sentence taken from the profile).
    pub said_right: bool,
    /// When it was kept or dropped.
    pub decided_at: Option<DateTime<Utc>>,
    /// Until when that can be undone.
    pub undo_until: Option<DateTime<Utc>>,
}

/// The proposals for one place of the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FactGroup {
    pub target: FactTarget,
    /// For a position: its place in `work` now.
    pub position: Option<u32>,
    /// For a position: its title and company.
    pub title: Option<String>,
    pub company: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`; no end while it is current.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub proposals: Vec<FactProposalView>,
}

/// What an import applied in the last day found, part by part.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvRead {
    pub applied_at: DateTime<Utc>,
    pub jobs: u32,
    pub achievements: u32,
    pub skills: u32,
    pub degrees: u32,
    pub languages: u32,
    pub certificates: u32,
    pub projects: u32,
}

/// The facts proposed for the profile: open ones, and those decided in the
/// last day (they can still be undone).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FactProposalsView {
    /// By position, newest position first; then the skills and the background.
    pub groups: Vec<FactGroup>,
    /// Waiting for Keep or Drop.
    pub open: u32,
    /// Kept in the last day.
    pub kept: u32,
    /// Dropped in the last day.
    pub dropped: u32,
    /// An import applied in the last day, and what it found.
    pub cv_read: Option<CvRead>,
    /// The last Keep all, while it can be undone in one go.
    pub batch: Option<KeptBatch>,
}

/// A Keep all: the facts it kept, undone together.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct KeptBatch {
    pub id: Uuid,
    /// Facts it added to the profile, still there.
    pub kept: u32,
    /// Until when it can be undone.
    pub undo_until: DateTime<Utc>,
}

// ---- making proposals -------------------------------------------------------------

/// What Tell me about you holds for `account`, while it is pending.
struct AboutRow {
    version: Option<i32>,
    portrait: Vec<Sentence>,
    choices: BTreeMap<String, Choice>,
    accuracy: Option<String>,
    changes: Vec<Change>,
}

impl AboutRow {
    fn known(&self) -> AboutKnown<'_> {
        AboutKnown {
            version: self.version,
            portrait: &self.portrait,
            choices: &self.choices,
            accuracy: self.accuracy.as_deref(),
            changes: &self.changes,
        }
    }
}

async fn about_of(
    conn: &mut PgConnection,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<AboutRow>> {
    let row = sqlx::query(
        "SELECT profile_version, portrait, choices, accuracy, changes FROM profile.about_you
          WHERE account_id = $1 AND expires_at > $2",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    // A pending state that cannot be read (an older shape) proposes nothing.
    let json = |column: &str| -> Option<serde_json::Value> { row.try_get(column).ok() };
    Ok(Some(AboutRow {
        version: row.try_get("profile_version")?,
        portrait: json("portrait")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        choices: json("choices")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        accuracy: row.try_get("accuracy")?,
        changes: json("changes")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
    }))
}

/// The draft of the import `account` applied most recently, while it is
/// kept, and when it was applied.
async fn applied_import(
    conn: &mut PgConnection,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<(Profile, DateTime<Utc>)>> {
    let row: Option<(sqlx::types::Json<Profile>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT draft, applied_at FROM profile.imports
          WHERE account_id = $1 AND status = 'applied' AND applied_at IS NOT NULL
            AND expires_at > $2 AND draft IS NOT NULL
          ORDER BY applied_at DESC LIMIT 1",
    )
    .bind(account)
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(draft, at)| (draft.0, at)))
}

/// What one round of proposing did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Proposed {
    /// New proposals.
    pub(crate) added: u32,
    /// Open proposals closed: the profile holds them now, or the person took
    /// them back in Tell me about you.
    pub(crate) closed: u32,
    /// Changes left out because their check failed.
    pub(crate) left_out: u32,
}

/// Propose for `account` what `profile` (at `version`) lacks of what `about`
/// and `cv` say, inside `conn`'s transaction: never one already proposed (in
/// any state, so a dropped fact is not proposed again), never one the profile
/// holds. An open proposal is closed when the profile holds it now, or when
/// what it came from no longer says it: the person took it back in Tell me
/// about you (only while its state is there to say so), or the CV read again
/// is another.
pub(crate) async fn propose_with(
    conn: &mut PgConnection,
    account: Uuid,
    profile: &Profile,
    version: i32,
    about: Option<&AboutKnown<'_>>,
    cv: Option<&Profile>,
    now: DateTime<Utc>,
) -> Result<Proposed> {
    let found = candidates(profile, version, about, cv);
    let wanted: Vec<Key> = found.candidates.iter().map(Candidate::key).collect();
    let portrait_current = about.is_some_and(|a| a.version == Some(version));
    let accurate = about.is_some_and(|a| a.accuracy == Some("yes"));
    let rows = sqlx::query(
        "SELECT id, target, role_key, text, state, origin FROM profile.fact_proposals
          WHERE account_id = $1",
    )
    .bind(account)
    .fetch_all(&mut *conn)
    .await?;
    let mut known: Vec<Key> = Vec::with_capacity(rows.len());
    let mut closing: Vec<Uuid> = Vec::new();
    for row in rows {
        let target = FactTarget::parse(row.try_get("target")?)
            .ok_or_else(|| Error::internal("a fact proposal of an unknown kind"))?;
        let role: Option<String> = row.try_get("role_key")?;
        let text: String = row.try_get("text")?;
        let state: String = row.try_get("state")?;
        let origin = Origin::parse(row.try_get("origin")?)
            .ok_or_else(|| Error::internal("a fact proposal of an unknown origin"))?;
        let key = key_of(target, role.as_deref(), &text);
        // A job removed since keeps its proposals out of sight, not deleted.
        let gone = target == FactTarget::Position
            && role
                .as_deref()
                .and_then(|r| position_by_key(profile, r))
                .is_none();
        if state == "open" && !gone {
            let held = already_held(
                profile,
                &Candidate {
                    target,
                    position: role.clone().map(|k| (0, k)),
                    text: text.clone(),
                    quote: String::new(),
                    source: FactSource::YouSaid,
                    origin,
                    basis: None,
                    area: Area::Results,
                },
            );
            let still = wanted.contains(&key);
            let taken_back = !still
                && match origin {
                    Origin::Change | Origin::AssumedRight | Origin::AssumedAccurate => {
                        about.is_some()
                    }
                    Origin::Known => about.is_some() && (!accurate || portrait_current),
                    Origin::Cv => cv.is_some(),
                };
            if held || taken_back {
                closing.push(row.try_get("id")?);
            }
        }
        known.push(key);
    }
    let mut added = 0;
    for candidate in found.candidates {
        let key = candidate.key();
        if known.contains(&key) {
            continue;
        }
        let role = candidate.position.as_ref().map(|(_, k)| k.as_str());
        sqlx::query(
            "INSERT INTO profile.fact_proposals
                (id, account_id, target, position, role_key, text, quote, source, basis, origin,
                 area, state, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'open', $12)",
        )
        .bind(Uuid::now_v7())
        .bind(account)
        .bind(candidate.target.as_str())
        .bind(
            candidate
                .position
                .as_ref()
                .map(|(at, _)| i32::try_from(*at).unwrap_or(i32::MAX)),
        )
        .bind(role)
        .bind(&candidate.text)
        .bind(&candidate.quote)
        .bind(candidate.source.as_str())
        .bind(&candidate.basis)
        .bind(candidate.origin.as_str())
        .bind(candidate.area.as_str())
        .bind(now)
        .execute(&mut *conn)
        .await?;
        known.push(key);
        added += 1;
    }
    if !closing.is_empty() {
        sqlx::query("DELETE FROM profile.fact_proposals WHERE id = ANY($1) AND state = 'open'")
            .bind(&closing)
            .execute(&mut *conn)
            .await?;
    }
    Ok(Proposed {
        added,
        closed: counted(closing.len()),
        left_out: found.left_out,
    })
}

/// Propose for `account` from what is stored now: its profile, Tell me about
/// you's pending state and the import it applied last.
pub(crate) async fn propose_for(
    conn: &mut PgConnection,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Proposed> {
    let row: Option<(sqlx::types::Json<Profile>, i32)> = sqlx::query_as(
        "SELECT data, version FROM profile.profiles WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((profile, version)) = row else {
        return Ok(Proposed::default());
    };
    let about = about_of(conn, account, now).await?;
    let cv = applied_import(conn, account, now).await?;
    propose_with(
        conn,
        account,
        &profile.0,
        version,
        about.as_ref().map(AboutRow::known).as_ref(),
        cv.as_ref().map(|(draft, _)| draft),
        now,
    )
    .await
}

/// The keys of the facts `account` dropped on Complete your profile: Tell me
/// about you's Save never writes them.
pub(crate) async fn dropped(conn: &mut PgConnection, account: Uuid) -> Result<Vec<Key>> {
    let rows: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT target, role_key, text FROM profile.fact_proposals
          WHERE account_id = $1 AND state = 'dropped'",
    )
    .bind(account)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(target, role, text)| {
            FactTarget::parse(&target).map(|t| key_of(t, role.as_deref(), &text))
        })
        .collect())
}

/// Queue proposing for `account` once `conn`'s transaction commits.
pub(crate) async fn queue(conn: &mut PgConnection, account: Uuid) -> Result<()> {
    jobs::enqueue(conn, &ProposeFacts { account }).await?;
    Ok(())
}

/// Propose the facts the app knows for one account. Free (no model) and
/// idempotent.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProposeFacts {
    /// Whose.
    pub account: Uuid,
}

impl Job for ProposeFacts {
    const NAME: &'static str = "profile.propose_facts";

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let mut tx = ctx.db.begin().await?;
        let done = propose_for(&mut tx, self.account, Utc::now()).await?;
        tx.commit().await?;
        // Counts only: never a fact.
        tracing::info!(
            added = done.added,
            closed = done.closed,
            left_out = done.left_out,
            "facts proposed"
        );
        Ok(())
    }
}

/// The marker of the one-off backfill.
const BACKFILL_MARKER: &str = "fact_proposals_backfill";

/// Propose the facts for every account that has a profile, once per instance:
/// the accounts that existed before Complete your profile. A marker keeps it
/// from running again; it is idempotent all the same.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackfillFacts {}

impl Job for BackfillFacts {
    const NAME: &'static str = "profile.backfill_facts";
    // Free and idempotent: the next start tries again.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let done = backfill(&ctx.db, Utc::now()).await?;
        tracing::info!(accounts = done, "facts proposed for the existing accounts");
        Ok(())
    }
}

/// The backfill; how many accounts it went through (none once done). An
/// account that fails is logged (its id only) and skipped, so the others get
/// their facts and the marker is written.
pub(crate) async fn backfill(db: &PgPool, now: DateTime<Utc>) -> Result<u32> {
    let done: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT done_at FROM profile.markers WHERE name = $1")
            .bind(BACKFILL_MARKER)
            .fetch_optional(db)
            .await?;
    if done.is_some() {
        return Ok(0);
    }
    let accounts: Vec<Uuid> =
        sqlx::query_scalar("SELECT account_id FROM profile.profiles ORDER BY account_id")
            .fetch_all(db)
            .await?;
    let mut count = 0;
    for account in accounts {
        let mut tx = db.begin().await?;
        match propose_for(&mut tx, account, now).await {
            Ok(_) => {
                tx.commit().await?;
                count += 1;
            }
            Err(error) => {
                tracing::warn!(%account, error = error.code(), "facts could not be proposed");
            }
        }
    }
    sqlx::query(
        "INSERT INTO profile.markers (name, done_at) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING",
    )
    .bind(BACKFILL_MARKER)
    .bind(now)
    .execute(db)
    .await?;
    Ok(count)
}

/// Queue the backfill to run now, unless one waits. Whether it was queued.
pub async fn schedule_backfill(db: &PgPool) -> Result<bool> {
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &BackfillFacts {}, Utc::now()).await?;
    tx.commit().await?;
    Ok(added)
}

// ---- reading them ---------------------------------------------------------------

#[derive(Debug, Clone)]
struct ProposalRow {
    id: Uuid,
    target: FactTarget,
    role_key: Option<String>,
    text: String,
    quote: String,
    source: FactSource,
    origin: Origin,
    basis: Option<String>,
    area: Area,
    state: FactProposalState,
    fact_id: Option<String>,
    batch: Option<Uuid>,
    decided_at: Option<DateTime<Utc>>,
}

impl ProposalRow {
    fn view(&self) -> FactProposalView {
        FactProposalView {
            id: self.id,
            text: self.text.clone(),
            quote: self.quote.clone(),
            source: self.source,
            basis: self.basis.clone(),
            area: self.area,
            state: self.state,
            said_right: self.origin == Origin::AssumedRight,
            decided_at: self.decided_at,
            undo_until: self.decided_at.map(|at| at + UNDO_LIFETIME),
        }
    }
}

fn row_of(row: &sqlx::postgres::PgRow) -> Result<ProposalRow> {
    let target: String = row.try_get("target")?;
    let source: String = row.try_get("source")?;
    let origin: String = row.try_get("origin")?;
    let area: String = row.try_get("area")?;
    let state: String = row.try_get("state")?;
    Ok(ProposalRow {
        id: row.try_get("id")?,
        target: FactTarget::parse(&target)
            .ok_or_else(|| Error::internal("a fact proposal of an unknown kind"))?,
        role_key: row.try_get("role_key")?,
        text: row.try_get("text")?,
        quote: row.try_get("quote")?,
        source: FactSource::parse(&source)
            .ok_or_else(|| Error::internal("a fact proposal of an unknown source"))?,
        origin: Origin::parse(&origin)
            .ok_or_else(|| Error::internal("a fact proposal of an unknown origin"))?,
        basis: row.try_get("basis")?,
        area: Area::parse(&area).ok_or_else(|| Error::internal("an unknown fact area"))?,
        state: FactProposalState::parse(&state),
        fact_id: row.try_get("fact_id")?,
        batch: row.try_get("batch")?,
        decided_at: row.try_get("decided_at")?,
    })
}

/// `account`'s proposals: open ones and those decided since `since`, locked
/// when `lock`.
async fn rows_of(
    conn: &mut PgConnection,
    account: Uuid,
    since: DateTime<Utc>,
    lock: bool,
) -> Result<Vec<ProposalRow>> {
    let sql = if lock {
        "SELECT id, target, role_key, text, quote, source, origin, basis, area, state, fact_id,
                batch, decided_at
           FROM profile.fact_proposals
          WHERE account_id = $1 AND (state = 'open' OR decided_at > $2)
          ORDER BY created_at, id FOR UPDATE"
    } else {
        "SELECT id, target, role_key, text, quote, source, origin, basis, area, state, fact_id,
                batch, decided_at
           FROM profile.fact_proposals
          WHERE account_id = $1 AND (state = 'open' OR decided_at > $2)
          ORDER BY created_at, id"
    };
    sqlx::query(sql)
        .bind(account)
        .bind(since)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(row_of)
        .collect()
}

/// The one proposal `id` of `account`, locked; 404 for anyone else's.
async fn one(tx: &mut PgConnection, account: Uuid, id: Uuid) -> Result<ProposalRow> {
    let row = sqlx::query(
        "SELECT id, target, role_key, text, quote, source, origin, basis, area, state, fact_id,
                batch, decided_at
           FROM profile.fact_proposals WHERE account_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(account)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;
    row_of(&row)
}

/// Whether a kept proposal is still what the profile says: one whose fact,
/// skill or words were removed by hand since is not shown as kept.
fn still_true(profile: &Profile, row: &ProposalRow) -> bool {
    let (FactProposalState::Kept, Some(fact)) = (row.state, row.fact_id.as_deref()) else {
        return true;
    };
    match row.target {
        FactTarget::Position => profile
            .work
            .iter()
            .any(|w| w.facts.iter().any(|f| f.id == fact)),
        FactTarget::Skills => profile
            .skills
            .iter()
            .any(|s| norm(&s.name) == norm(&row.text)),
        FactTarget::Background => profile
            .basics
            .background
            .as_deref()
            .is_some_and(|b| norm(b).contains(&norm(&row.text))),
    }
}

/// The position `key` names in `profile` now.
fn position_of(profile: &Profile, key: Option<&str>) -> Option<usize> {
    position_by_key(profile, key?)
}

fn counted(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The order positions are shown in: the current and latest first.
fn newest_first(profile: &Profile, a: usize, b: usize) -> std::cmp::Ordering {
    let key = |at: usize| {
        let work = &profile.work[at];
        (
            work.end_date.clone().unwrap_or_else(|| "9999".to_owned()),
            work.start_date.clone().unwrap_or_default(),
        )
    };
    key(b).cmp(&key(a)).then(a.cmp(&b))
}

fn cv_read_of(draft: &Profile, applied_at: DateTime<Utc>) -> CvRead {
    CvRead {
        applied_at,
        jobs: counted(draft.work.len()),
        achievements: counted(draft.work.iter().map(|w| w.highlights.len()).sum()),
        skills: counted(draft.skills.len()),
        degrees: counted(draft.education.len()),
        languages: counted(draft.languages.len()),
        certificates: counted(draft.certificates.len()),
        projects: counted(draft.projects.len()),
    }
}

async fn view_for(state: &AppState, account: Uuid) -> Result<FactProposalsView> {
    let now = state.clock.now();
    let mut conn = state.db.acquire().await?;
    let profile = completion::current(&state.db, account)
        .await?
        .map(|c| c.profile)
        .unwrap_or_default();
    let rows: Vec<ProposalRow> = rows_of(&mut conn, account, now - UNDO_LIFETIME, false)
        .await?
        .into_iter()
        .filter(|row| still_true(&profile, row))
        .collect();
    let mut positions: BTreeMap<usize, Vec<FactProposalView>> = BTreeMap::new();
    let mut skills = Vec::new();
    let mut background = Vec::new();
    let mut shown: Vec<&ProposalRow> = Vec::new();
    for row in &rows {
        match row.target {
            FactTarget::Position => {
                // A position removed since: not shown.
                if let Some(at) = position_of(&profile, row.role_key.as_deref()) {
                    positions.entry(at).or_default().push(row.view());
                    shown.push(row);
                }
            }
            FactTarget::Skills => {
                skills.push(row.view());
                shown.push(row);
            }
            FactTarget::Background => {
                background.push(row.view());
                shown.push(row);
            }
        }
    }
    let mut order: Vec<usize> = positions.keys().copied().collect();
    order.sort_by(|a, b| newest_first(&profile, *a, *b));
    let mut groups: Vec<FactGroup> = order
        .into_iter()
        .filter_map(|at| {
            let work = profile.work.get(at)?;
            Some(FactGroup {
                target: FactTarget::Position,
                position: u32::try_from(at).ok(),
                title: work.position.clone(),
                company: work.name.clone(),
                start_date: work.start_date.clone(),
                end_date: work.end_date.clone(),
                proposals: positions.remove(&at).unwrap_or_default(),
            })
        })
        .collect();
    for (target, proposals) in [
        (FactTarget::Skills, skills),
        (FactTarget::Background, background),
    ] {
        if !proposals.is_empty() {
            groups.push(FactGroup {
                target,
                position: None,
                title: None,
                company: None,
                start_date: None,
                end_date: None,
                proposals,
            });
        }
    }
    let count =
        |state: FactProposalState| counted(shown.iter().filter(|p| p.state == state).count());
    let (open, kept, dropped) = (
        count(FactProposalState::Open),
        count(FactProposalState::Kept),
        count(FactProposalState::Dropped),
    );
    // The last Keep all, while its facts are on the profile and a day has not
    // passed.
    let batch = shown
        .iter()
        .filter(|p| p.state == FactProposalState::Kept)
        .filter_map(|p| Some((p.batch?, p.decided_at?)))
        .max_by_key(|(_, at)| *at)
        .map(|(id, at)| KeptBatch {
            id,
            kept: counted(
                shown
                    .iter()
                    .filter(|p| p.state == FactProposalState::Kept && p.batch == Some(id))
                    .count(),
            ),
            undo_until: at + UNDO_LIFETIME,
        });
    let cv_read = applied_import(&mut conn, account, now)
        .await?
        .filter(|(_, at)| *at > now - CV_READ_SHOWN)
        .map(|(draft, at)| cv_read_of(&draft, at));
    Ok(FactProposalsView {
        groups,
        open,
        kept,
        dropped,
        cv_read,
        batch,
    })
}

/// The facts proposed for the profile.
///
/// What the app already knows that the profile does not hold: what the person
/// told and confirmed in Tell me about you, what their own website says, what
/// a CV read again says. Open ones, and those kept or dropped in the last day
/// (still undoable), grouped by position, newest first, then skills and
/// background. Never calls a model.
#[utoipa::path(
    get,
    path = "/api/v1/profile/facts/proposals",
    tag = "profile",
    operation_id = "profile.fact_proposals",
    security(("session" = [])),
    responses(
        (status = 200, description = "The proposals.", body = FactProposalsView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<FactProposalsView>> {
    Ok(Json(view_for(&state, principal.account_id).await?))
}

// ---- deciding --------------------------------------------------------------------

fn not_open() -> Error {
    Error::Conflict(
        "proposal_not_open",
        "This fact was already kept or dropped. Undo it first.".to_owned(),
    )
}

fn profile_changed() -> Error {
    Error::Conflict(
        "profile_changed",
        "That job changed on your profile since this fact was found.".to_owned(),
    )
}

/// The profile, locked.
async fn locked_profile(tx: &mut PgConnection, account: Uuid) -> Result<Profile> {
    let row: Option<(sqlx::types::Json<Profile>,)> =
        sqlx::query_as("SELECT data FROM profile.profiles WHERE account_id = $1 FOR UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    Ok(row.ok_or(Error::NotFound)?.0.0)
}

/// What Undo takes back of a kept skill or background: what Keep added.
const ADDED: &str = "added";
/// A skill Keep added that the person had said was not theirs: Undo says so
/// again.
const ADDED_UNDISMISSED: &str = "added_undismissed";

/// What Keep did with one proposal.
enum Kept {
    /// Wrote it: the fact's id, or [`ADDED`] / [`ADDED_UNDISMISSED`].
    Wrote(String),
    /// The profile had it already: nothing was written.
    Held,
}

/// Write `proposal` into `profile`. `Err(profile_changed)` when its position is
/// gone. `dismissed` are the skills the person said are not theirs.
fn keep_into(
    profile: &mut Profile,
    proposal: &ProposalRow,
    dismissed: &[String],
    now: DateTime<Utc>,
) -> Result<Kept> {
    match proposal.target {
        FactTarget::Position => {
            let at =
                position_of(profile, proposal.role_key.as_deref()).ok_or_else(profile_changed)?;
            let work = &mut profile.work[at];
            if propose::holds(work, &proposal.text) {
                return Ok(Kept::Held);
            }
            let id = proposal.id.to_string();
            work.facts.push(ProfileFact {
                id: id.clone(),
                text: proposal.text.clone(),
                quote: proposal.quote.clone(),
                area: proposal.area,
                kept_at: now,
                status: proposal.source.status(),
                basis: proposal.basis.clone(),
            });
            Ok(Kept::Wrote(id))
        }
        FactTarget::Skills => {
            if profile
                .skills
                .iter()
                .any(|s| applicamp_kernel::tech::same_skill(&s.name, &proposal.text))
            {
                return Ok(Kept::Held);
            }
            profile.skills.push(Skill {
                name: proposal.text.trim().to_owned(),
                level: None,
            });
            let undismissed = dismissed.contains(&proposal.text.trim().to_lowercase());
            Ok(Kept::Wrote(
                if undismissed {
                    ADDED_UNDISMISSED
                } else {
                    ADDED
                }
                .to_owned(),
            ))
        }
        FactTarget::Background => {
            let words = proposal.text.trim();
            let background = &mut profile.basics.background;
            match background.as_deref().map(str::trim) {
                Some(had) if norm(had).contains(&norm(words)) => return Ok(Kept::Held),
                Some(had) if !had.is_empty() => {
                    *background = Some(format!("{had}\n\n{words}"));
                }
                _ => *background = Some(words.to_owned()),
            }
            Ok(Kept::Wrote(ADDED.to_owned()))
        }
    }
}

/// Whether `profile`, as saved, still has what Keep wrote for `proposal`.
fn wrote_it(profile: &Profile, proposal: &ProposalRow, fact: &str) -> bool {
    match proposal.target {
        FactTarget::Position => profile
            .work
            .iter()
            .any(|w| w.facts.iter().any(|f| f.id == fact)),
        FactTarget::Skills => profile
            .skills
            .iter()
            .any(|s| norm(&s.name) == norm(&proposal.text)),
        FactTarget::Background => profile.basics.background.is_some(),
    }
}

/// What a Keep did, counted.
struct KeepOutcome {
    /// Facts written to the profile.
    kept: u32,
}

/// Keep `proposals` (all open): at most one profile write, one new version,
/// and none when nothing changed. Those whose position is gone are refused
/// when `strict`, else left as they are. One the profile held already is
/// closed, not kept; one the profile's tidying dropped is left open.
async fn keep(
    state: &AppState,
    account: Uuid,
    proposals: &[ProposalRow],
    strict: bool,
    batch: Option<Uuid>,
    tx: &mut PgConnection,
) -> Result<KeepOutcome> {
    let now = state.clock.now();
    let mut profile = locked_profile(tx, account).await?;
    let dismissed: Vec<String> =
        sqlx::query_scalar("SELECT name_key FROM profile.skill_dismissals WHERE account_id = $1")
            .bind(account)
            .fetch_all(&mut *tx)
            .await?;
    let before = profile.clone();
    let mut wrote: Vec<(&ProposalRow, String)> = Vec::new();
    let mut held: Vec<Uuid> = Vec::new();
    for proposal in proposals {
        match keep_into(&mut profile, proposal, &dismissed, now) {
            Ok(Kept::Wrote(fact)) => wrote.push((proposal, fact)),
            Ok(Kept::Held) => held.push(proposal.id),
            Err(error) if strict => return Err(error),
            Err(_) => {}
        }
    }
    if profile != before {
        let profile = profile.tidied();
        profile.validate()?;
        wrote.retain(|(proposal, fact)| wrote_it(&profile, proposal, fact));
        completion::write_profile(state, tx, account, &profile, Some("answer")).await?;
    }
    for (proposal, fact) in &wrote {
        sqlx::query(
            "UPDATE profile.fact_proposals
                SET state = 'kept', decided_at = $2, fact_id = $3, batch = $4
              WHERE id = $1",
        )
        .bind(proposal.id)
        .bind(now)
        .bind(fact)
        .bind(batch)
        .execute(&mut *tx)
        .await?;
    }
    if !held.is_empty() {
        sqlx::query("DELETE FROM profile.fact_proposals WHERE id = ANY($1) AND state = 'open'")
            .bind(&held)
            .execute(&mut *tx)
            .await?;
    }
    // A skill kept is the person's: an earlier "not mine" is forgotten (and
    // said again if the Keep is undone).
    let skills: Vec<String> = wrote
        .iter()
        .filter(|(p, fact)| p.target == FactTarget::Skills && fact == ADDED_UNDISMISSED)
        .map(|(p, _)| p.text.trim().to_lowercase())
        .collect();
    if !skills.is_empty() {
        sqlx::query(
            "DELETE FROM profile.skill_dismissals WHERE account_id = $1 AND name_key = ANY($2)",
        )
        .bind(account)
        .bind(&skills)
        .execute(&mut *tx)
        .await?;
    }
    Ok(KeepOutcome {
        kept: counted(wrote.len()),
    })
}

/// Keep a fact.
///
/// Writes it on its position (with its quote; a confirmed assumption keeps
/// what it was based on), adds the skill or adds to the background: one new
/// profile version. Can be undone for a day. A fact the profile already holds
/// is closed, and nothing is written.
#[utoipa::path(
    post,
    path = "/api/v1/profile/facts/proposals/{id}/keep",
    tag = "profile",
    operation_id = "profile.fact_keep",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The proposal.")),
    responses(
        (status = 200, description = "The proposals, after it was kept.", body = FactProposalsView),
        (status = 400, description = "`invalid_profile`: the profile would break a limit",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_not_open`: already kept or dropped; \
            `profile_changed`: its job is no longer on the profile", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn keep_one(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<FactProposalsView>> {
    let account = principal.account_id;
    let mut tx = state.db.begin().await?;
    let proposal = one(&mut tx, account, id).await?;
    if proposal.state != FactProposalState::Open {
        return Err(not_open());
    }
    let done = keep(
        &state,
        account,
        std::slice::from_ref(&proposal),
        true,
        None,
        &mut tx,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(kept = done.kept, "a proposed fact kept");
    Ok(Json(view_for(&state, account).await?))
}

/// Keep every open fact.
///
/// One profile write, one new version, and one Undo for all of them for a day
/// (`…/batches/{id}/undo`). Facts whose job is no longer on the profile are
/// left as they are.
#[utoipa::path(
    post,
    path = "/api/v1/profile/facts/proposals/keep-all",
    tag = "profile",
    operation_id = "profile.fact_keep_all",
    security(("session" = [])),
    responses(
        (status = 200, description = "The proposals, after they were kept.", body = FactProposalsView),
        (status = 400, description = "`invalid_profile`: the profile would break a limit",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn keep_all(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
) -> Result<Json<FactProposalsView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let open: Vec<ProposalRow> = rows_of(&mut tx, account, now, true)
        .await?
        .into_iter()
        .filter(|p| p.state == FactProposalState::Open)
        .collect();
    let kept = if open.is_empty() {
        0
    } else {
        keep(&state, account, &open, false, Some(Uuid::now_v7()), &mut tx)
            .await?
            .kept
    };
    tx.commit().await?;
    tracing::info!(kept, "proposed facts kept");
    Ok(Json(view_for(&state, account).await?))
}

/// Drop a fact: it is not written, not proposed again, and never written by
/// Tell me about you's Save. Can be undone for a day.
#[utoipa::path(
    post,
    path = "/api/v1/profile/facts/proposals/{id}/drop",
    tag = "profile",
    operation_id = "profile.fact_drop",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The proposal.")),
    responses(
        (status = 200, description = "The proposals, after it was dropped.", body = FactProposalsView),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_not_open`: already kept", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn drop_one(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<FactProposalsView>> {
    let account = principal.account_id;
    let mut tx = state.db.begin().await?;
    let proposal = one(&mut tx, account, id).await?;
    match proposal.state {
        FactProposalState::Open => {
            sqlx::query(
                "UPDATE profile.fact_proposals SET state = 'dropped', decided_at = $2 WHERE id = $1",
            )
            .bind(id)
            .bind(state.clock.now())
            .execute(&mut *tx)
            .await?;
        }
        FactProposalState::Dropped => {}
        FactProposalState::Kept => return Err(not_open()),
    }
    tx.commit().await?;
    Ok(Json(view_for(&state, account).await?))
}

/// Take back from `profile` exactly what Keep wrote for `proposal`: the fact
/// it wrote, the skill it added, the words it added to the background (only
/// while the background still ends with them). The skills to say "not mine"
/// about again.
fn take_back(profile: &mut Profile, proposal: &ProposalRow) -> Vec<String> {
    let Some(fact) = proposal.fact_id.as_deref() else {
        return Vec::new();
    };
    match proposal.target {
        FactTarget::Position => {
            for work in &mut profile.work {
                work.facts.retain(|f| f.id != fact);
            }
            Vec::new()
        }
        FactTarget::Skills => {
            if let Some(at) = profile
                .skills
                .iter()
                .position(|s| norm(&s.name) == norm(&proposal.text))
            {
                profile.skills.remove(at);
            }
            if fact == ADDED_UNDISMISSED {
                vec![proposal.text.trim().to_lowercase()]
            } else {
                Vec::new()
            }
        }
        FactTarget::Background => {
            let words = proposal.text.trim();
            if let Some(background) = profile.basics.background.as_deref() {
                let background = background.trim();
                if background == words {
                    profile.basics.background = None;
                } else if let Some(rest) = background.strip_suffix(words)
                    && let Some(rest) = rest.strip_suffix("\n\n")
                {
                    profile.basics.background = Some(rest.trim_end().to_owned());
                }
            }
            Vec::new()
        }
    }
}

/// Undo `proposals` (all decided): kept ones taken back in one profile write
/// (none when nothing changed), all of them open again.
async fn undo_all(
    state: &AppState,
    account: Uuid,
    proposals: &[ProposalRow],
    tx: &mut PgConnection,
) -> Result<()> {
    let kept: Vec<&ProposalRow> = proposals
        .iter()
        .filter(|p| p.state == FactProposalState::Kept)
        .collect();
    if !kept.is_empty() {
        let mut profile = locked_profile(tx, account).await?;
        let before = profile.clone();
        let mut dismiss: Vec<String> = Vec::new();
        for proposal in kept {
            dismiss.extend(take_back(&mut profile, proposal));
        }
        if profile != before {
            let profile = profile.tidied();
            profile.validate()?;
            completion::write_profile(state, tx, account, &profile, None).await?;
        }
        for name in dismiss {
            sqlx::query(
                "INSERT INTO profile.skill_dismissals (account_id, name_key, created_at)
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(account)
            .bind(name)
            .bind(state.clock.now())
            .execute(&mut *tx)
            .await?;
        }
    }
    let ids: Vec<Uuid> = proposals.iter().map(|p| p.id).collect();
    sqlx::query(
        "UPDATE profile.fact_proposals
            SET state = 'open', decided_at = NULL, fact_id = NULL, batch = NULL
          WHERE id = ANY($1)",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Undo a Keep or a Drop, within a day.
///
/// A kept fact is taken off its position again (the skill it added out of the
/// skills, its words out of the background while it still ends with them):
/// one new profile version. A dropped one is open again.
#[utoipa::path(
    post,
    path = "/api/v1/profile/facts/proposals/{id}/undo",
    tag = "profile",
    operation_id = "profile.fact_undo",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The proposal.")),
    responses(
        (status = 200, description = "The proposals, after the undo.", body = FactProposalsView),
        (status = 400, description = "`invalid_profile`: the profile would break a limit",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`proposal_not_decided`: nothing to undo; \
            `undo_expired`: decided more than a day ago", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn undo(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<FactProposalsView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let proposal = one(&mut tx, account, id).await?;
    let Some(decided_at) = proposal.decided_at else {
        return Err(Error::Conflict(
            "proposal_not_decided",
            "This fact was not kept or dropped.".to_owned(),
        ));
    };
    if decided_at + UNDO_LIFETIME <= now {
        return Err(undo_expired());
    }
    undo_all(&state, account, std::slice::from_ref(&proposal), &mut tx).await?;
    tx.commit().await?;
    Ok(Json(view_for(&state, account).await?))
}

fn undo_expired() -> Error {
    Error::Conflict(
        "undo_expired",
        "This can only be undone for a day.".to_owned(),
    )
}

/// Undo a Keep all, within a day.
///
/// Every fact it kept is taken back, in one profile write, and waits again.
#[utoipa::path(
    post,
    path = "/api/v1/profile/facts/proposals/batches/{id}/undo",
    tag = "profile",
    operation_id = "profile.fact_undo_batch",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The Keep all (`batch.id`).")),
    responses(
        (status = 200, description = "The proposals, after the undo.", body = FactProposalsView),
        (status = 400, description = "`invalid_profile`: the profile would break a limit",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such Keep all", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`undo_expired`: kept more than a day ago", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn undo_batch(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<FactProposalsView>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let rows = sqlx::query(
        "SELECT id, target, role_key, text, quote, source, origin, basis, area, state, fact_id,
                batch, decided_at
           FROM profile.fact_proposals
          WHERE account_id = $1 AND batch = $2 AND state = 'kept' FOR UPDATE",
    )
    .bind(account)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let kept: Vec<ProposalRow> = rows.iter().map(row_of).collect::<Result<_>>()?;
    let Some(decided_at) = kept.iter().filter_map(|p| p.decided_at).max() else {
        return Err(Error::NotFound);
    };
    if decided_at + UNDO_LIFETIME <= now {
        return Err(undo_expired());
    }
    undo_all(&state, account, &kept, &mut tx).await?;
    tx.commit().await?;
    tracing::info!(facts = kept.len(), "a Keep all undone");
    Ok(Json(view_for(&state, account).await?))
}
