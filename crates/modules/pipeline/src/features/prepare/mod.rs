//! Prepare by itself (#70, ADR 0010): a shortlisted job gets ready, waiting
//! only on the person.
//!
//! When an opportunity moves from Found to Shortlisted or later, and the
//! person's Autonomy goal is "Prepare" (or not answered), one background job
//! ([`Prepare`]) runs the steps a person would click, in order:
//!
//! 1. **read**: find the company's site ([`brief`]'s finder: never a search
//!    engine), read it and compare the job (the same calls as Read). Without a
//!    site it waits on the person to type one.
//! 2. it **waits on the answers** while a question is open or a fact taken
//!    from an answer waits for Keep or Drop;
//! 3. **draft**: draft the CV (the same call as "Draft the CV"), then wait on
//!    its approval;
//! 4. **letter**: once the CV is approved, write the cover letter and the
//!    job's answers, then wait on their approval.
//!
//! What it waits on is checked again whenever the person acts on the job
//! (opening its brief, typing the site, answering, approving): [`nudge`]
//! queues the next step when the wait is over. Nothing runs on a timer.
//!
//! The person stays in control: every step done by itself is recorded with
//! what it cost; Stop cancels what is still to run, and so does moving the
//! job back to Found or "Not for me", or the goal "Find only"; a failed step
//! is never retried by itself (Try again is "Prepare by itself"). Caps and the
//! AI budget are checked by each step as on a click; a step they refuse stops
//! the preparation with their code, and the page offers the step's button.
//! Approving, answering and moving the job stay the person's.

#[cfg(test)]
mod tests;

use applicamp_ai::Role;
use applicamp_kernel::{
    AppState, Error, Job, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    PipelineDeps,
    features::{
        brief::{self, Brief},
        cv::{self, letter},
    },
    ports::Autonomy,
};

// ---- views ----------------------------------------------------------------

/// Where a job's preparation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreparationState {
    /// Its next step is about to run.
    Queued,
    /// A step is running.
    Running,
    /// It waits on the person (`waiting_on`).
    Waiting,
    /// Everything is ready: the letter was approved.
    Done,
    /// Stopped: by the person, by moving the job, by the goal "Find only", or
    /// by a cap or a spending limit (`reason`).
    Stopped,
    /// A step failed (`reason`); it is not retried by itself.
    Failed,
}

impl PreparationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "waiting" => Self::Waiting,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Stopped,
        }
    }
}

/// A step of the preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreparationStep {
    /// Find the site, read the company, compare the job.
    Read,
    /// Draft the CV.
    Draft,
    /// Write the cover letter and the job's answers.
    Letter,
}

impl PreparationStep {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::Letter => "letter",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "draft" => Self::Draft,
            "letter" => Self::Letter,
            _ => Self::Read,
        }
    }
}

/// What a preparation waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitingOn {
    /// The company's website: none was found.
    Site,
    /// Open questions, or facts from answers waiting for Keep or Drop.
    Answers,
    /// Approving the CV.
    CvApproval,
    /// Approving the cover letter.
    LetterApproval,
}

impl WaitingOn {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Answers => "answers",
            Self::CvApproval => "cv_approval",
            Self::LetterApproval => "letter_approval",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "site" => Some(Self::Site),
            "answers" => Some(Self::Answers),
            "cv_approval" => Some(Self::CvApproval),
            "letter_approval" => Some(Self::LetterApproval),
            _ => None,
        }
    }
}

/// A step done by itself.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreparedStep {
    /// `site` (found the company's website, no model), `read` (read the
    /// company), `compare` (compared the job), `draft` (drafted the CV) or
    /// `letter` (wrote the letter).
    pub step: String,
    pub at: DateTime<Utc>,
    /// About what it cost, in euros.
    pub eur: f64,
}

/// What the app does by itself for a job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Preparation {
    pub state: PreparationState,
    /// The step to run next, or the one that stopped or failed.
    pub step: PreparationStep,
    /// While waiting: what on.
    pub waiting_on: Option<WaitingOn>,
    /// Why it stopped or failed: a problem code (`cv_drafts_daily_cap`,
    /// `ai_budget_reached`, `find_only`, `stopped`, `not_shortlisted`…).
    pub reason: Option<String>,
    /// The steps done by itself, oldest first.
    pub done: Vec<PreparedStep>,
    /// About what they cost, in euros.
    pub spent_eur: f64,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct Row {
    account_id: Uuid,
    state: String,
    step: String,
    waiting_on: Option<String>,
    reason: Option<String>,
    done: sqlx::types::Json<Vec<PreparedStep>>,
    updated_at: DateTime<Utc>,
}

async fn row_of(db: &PgPool, opp: Uuid) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "SELECT account_id, state, step, waiting_on, reason, done, updated_at
           FROM pipeline.preparations WHERE opportunity_id = $1",
    )
    .bind(opp)
    .fetch_optional(db)
    .await?)
}

/// What the app does by itself for opportunity `opp`; none until it started.
pub(crate) async fn view(db: &PgPool, opp: Uuid) -> Result<Option<Preparation>> {
    Ok(row_of(db, opp).await?.map(|row| Preparation {
        state: PreparationState::parse(&row.state),
        step: PreparationStep::parse(&row.step),
        waiting_on: row.waiting_on.as_deref().and_then(WaitingOn::parse),
        reason: row.reason,
        spent_eur: row.done.0.iter().map(|s| s.eur).sum(),
        done: row.done.0,
        updated_at: row.updated_at,
    }))
}

// ---- whether it may run ---------------------------------------------------

fn ai_ready(deps: &PipelineDeps) -> bool {
    deps.ai
        .as_ref()
        .is_some_and(|ai| ai.is_configured(Role::Complete))
}

/// Whether the app prepares `account`'s shortlisted jobs by itself: a model is
/// set up and their Autonomy goal is not "Find only" (not answered counts as
/// "Prepare").
pub(crate) async fn by_itself(deps: &PipelineDeps, db: &PgPool, account: Uuid) -> Result<bool> {
    let Some(neighbours) = &deps.neighbours else {
        return Ok(false);
    };
    if !ai_ready(deps) {
        return Ok(false);
    }
    Ok(neighbours.autonomy(db, account).await? != Some(Autonomy::FindOnly))
}

/// Why `opp` may not be prepared by itself now, as a code; none when it may.
async fn refusal(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
) -> Result<Option<&'static str>> {
    let on: Option<(String, bool)> = sqlx::query_as(
        "SELECT stage, dismissed FROM pipeline.opportunities WHERE id = $1 AND account_id = $2",
    )
    .bind(opp)
    .bind(account)
    .fetch_optional(db)
    .await?;
    let Some((stage, dismissed)) = on else {
        return Ok(Some("not_found"));
    };
    if stage == "found" || dismissed {
        return Ok(Some("not_shortlisted"));
    }
    if !ai_ready(deps) {
        return Ok(Some(applicamp_ai::AI_UNAVAILABLE));
    }
    if let Some(neighbours) = &deps.neighbours
        && neighbours.autonomy(db, account).await? == Some(Autonomy::FindOnly)
    {
        return Ok(Some("find_only"));
    }
    Ok(None)
}

// ---- where it stands ------------------------------------------------------

/// What comes next for a preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Run this step.
    Run(PreparationStep),
    /// Wait on the person.
    Wait(WaitingOn),
    /// Everything is ready.
    Done,
}

/// The latest CV (or, with `letter`, cover letter) draft's status (`draft`
/// or `approved`), if any.
async fn latest_status(db: &PgPool, opp: Uuid, letter: bool) -> Result<Option<String>> {
    let sql = if letter {
        "SELECT status FROM pipeline.letter_drafts
          WHERE opportunity_id = $1 AND status IN ('draft', 'approved') ORDER BY n DESC LIMIT 1"
    } else {
        "SELECT status FROM pipeline.cv_drafts
          WHERE opportunity_id = $1 AND status IN ('draft', 'approved') ORDER BY n DESC LIMIT 1"
    };
    Ok(sqlx::query_scalar(sql).bind(opp).fetch_optional(db).await?)
}

/// Whether the job's questions still wait on the person.
async fn answers_wait(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    now: DateTime<Utc>,
) -> Result<bool> {
    let (open, proposals) = brief::questions_left(deps, db, account, opp).await?;
    if open > 0 {
        return Ok(true);
    }
    let waiting = match &deps.neighbours {
        Some(neighbours) if !proposals.is_empty() => {
            neighbours
                .facts_waiting(db, account, &proposals, now)
                .await?
        }
        _ => 0,
    };
    Ok(waiting > 0)
}

/// Where the CV and the letter stand: what comes after the read.
async fn after_read(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    now: DateTime<Utc>,
) -> Result<Next> {
    match latest_status(db, opp, true).await?.as_deref() {
        Some("approved") => return Ok(Next::Done),
        Some(_) => return Ok(Next::Wait(WaitingOn::LetterApproval)),
        None => {}
    }
    match latest_status(db, opp, false).await?.as_deref() {
        Some("approved") => return Ok(Next::Run(PreparationStep::Letter)),
        Some(_) => return Ok(Next::Wait(WaitingOn::CvApproval)),
        None => {}
    }
    if answers_wait(deps, db, account, opp, now).await? {
        return Ok(Next::Wait(WaitingOn::Answers));
    }
    Ok(Next::Run(PreparationStep::Draft))
}

// ---- changing it ----------------------------------------------------------

/// Queue `step` of `opp`'s preparation (and the job that runs it) in `conn`'s
/// transaction, unless a step is already queued or running. Whether it did.
async fn queue(
    conn: &mut PgConnection,
    account: Uuid,
    opp: Uuid,
    step: PreparationStep,
    locale: Option<&str>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let queued: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO pipeline.preparations
            (opportunity_id, account_id, state, step, locale, updated_at)
         VALUES ($1, $2, 'queued', $3, $4, $5)
         ON CONFLICT (opportunity_id) DO UPDATE
            SET state = 'queued', step = excluded.step, waiting_on = NULL, reason = NULL,
                locale = coalesce(excluded.locale, pipeline.preparations.locale),
                updated_at = excluded.updated_at
          WHERE pipeline.preparations.state NOT IN ('queued', 'running')
         RETURNING opportunity_id",
    )
    .bind(opp)
    .bind(account)
    .bind(step.as_str())
    .bind(locale.map(|l| l.chars().take(35).collect::<String>()))
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    if queued.is_none() {
        return Ok(false);
    }
    if step == PreparationStep::Letter {
        // The CV was just approved: the letter waits while the approval can
        // still be undone.
        jobs::enqueue_at(
            &mut *conn,
            &Prepare { opportunity: opp },
            now + cv::UNDO_WINDOW,
        )
        .await?;
    } else {
        jobs::enqueue(&mut *conn, &Prepare { opportunity: opp }).await?;
    }
    Ok(true)
}

/// Stop `opp`'s preparation with `reason`, in `conn`'s transaction: what is
/// queued does not run, and a running step's outcome is not kept as next.
pub(crate) async fn cancel(
    conn: &mut PgConnection,
    opp: Uuid,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE pipeline.preparations
            SET state = 'stopped', reason = $2, waiting_on = NULL, updated_at = $3
          WHERE opportunity_id = $1 AND state IN ('queued', 'running', 'waiting')",
    )
    .bind(opp)
    .bind(reason)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// A job was just shortlisted: start its preparation from the start when the
/// app prepares by itself. Never fails the request that shortlisted it.
pub(crate) async fn begin(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    locale: Option<&str>,
    now: DateTime<Utc>,
) {
    let started = async {
        if !by_itself(deps, db, account).await? {
            return Ok(false);
        }
        let mut tx = db.begin().await?;
        let queued = queue(&mut tx, account, opp, PreparationStep::Read, locale, now).await?;
        tx.commit().await?;
        Ok::<bool, Error>(queued)
    }
    .await;
    match started {
        Ok(true) => tracing::info!(opportunity = %opp, "a preparation started"),
        Ok(false) => {}
        Err(e) => tracing::warn!(error = e.code(), "a preparation could not start"),
    }
}

/// The person acted on `opp` (opened its brief, typed the site, answered,
/// approved): start the preparation of a shortlisted job that was never
/// prepared, or queue its next step when what it waited on is done. No model
/// call and no fetch here. Never fails the request it rides on.
pub(crate) async fn nudge(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    now: DateTime<Utc>,
) {
    nudge_in(deps, db, account, opp, None, now).await;
}

/// [`nudge`], with the language the person uses (`locale`), for the
/// questions of a preparation it starts.
pub(crate) async fn nudge_in(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    locale: Option<&str>,
    now: DateTime<Utc>,
) {
    if let Err(e) = try_nudge(deps, db, account, opp, locale, now).await {
        tracing::warn!(error = e.code(), "a preparation could not move on");
    }
}

async fn try_nudge(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    locale: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let row = row_of(db, opp).await?;
    let waiting = match &row {
        // Never prepared: start it, when it may.
        None => {
            if refusal(deps, db, account, opp).await?.is_none() {
                let mut tx = db.begin().await?;
                queue(&mut tx, account, opp, PreparationStep::Read, locale, now).await?;
                tx.commit().await?;
            }
            return Ok(());
        }
        Some(row) if row.account_id == account && row.state == "waiting" => {
            row.waiting_on.as_deref().and_then(WaitingOn::parse)
        }
        Some(_) => return Ok(()),
    };
    if refusal(deps, db, account, opp).await?.is_some() {
        return Ok(());
    }
    let next = match waiting {
        // The site: only once one is known (typed, or found by a later read).
        Some(WaitingOn::Site) => {
            if brief::site_known(db, account, opp).await? {
                Next::Run(PreparationStep::Read)
            } else {
                Next::Wait(WaitingOn::Site)
            }
        }
        _ => after_read(deps, db, account, opp, now).await?,
    };
    match next {
        Next::Run(step) => {
            let mut tx = db.begin().await?;
            queue(&mut tx, account, opp, step, locale, now).await?;
            tx.commit().await?;
        }
        Next::Wait(on) if Some(on) != waiting => {
            settle(db, opp, PreparationState::Waiting, Some(on), None, now).await?
        }
        Next::Done => settle(db, opp, PreparationState::Done, None, None, now).await?,
        Next::Wait(_) => {}
    }
    Ok(())
}

/// Write where a waiting preparation is now (no step runs).
async fn settle(
    db: &PgPool,
    opp: Uuid,
    state: PreparationState,
    waiting_on: Option<WaitingOn>,
    reason: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE pipeline.preparations
            SET state = $2, waiting_on = $3, reason = $4, updated_at = $5
          WHERE opportunity_id = $1 AND state = 'waiting'",
    )
    .bind(opp)
    .bind(state.as_str())
    .bind(waiting_on.map(WaitingOn::as_str))
    .bind(reason)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

// ---- the job --------------------------------------------------------------

/// Runs a job's preparation, step after step, until it waits on the person,
/// is done, stops or fails. Enqueued with every step queued; a job that finds
/// nothing queued does nothing.
#[derive(Debug, Serialize, Deserialize)]
pub struct Prepare {
    /// The opportunity.
    pub opportunity: Uuid,
}

impl Job for Prepare {
    const NAME: &'static str = "pipeline.prepare";
    // Never retried: a failure is shown on the job, and a retry could only
    // spend again.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<PipelineDeps>()
            .ok_or_else(|| JobError::Permanent("the pipeline module is not composed".into()))?;
        run(deps, &ctx.db, self.opportunity)
            .await
            .map_err(|e| JobError::Permanent(e.code().to_owned()))
    }
}

/// Codes that stop a preparation (a cap, a limit, something to do first);
/// anything else fails it.
const STOPS: &[&str] = &[
    "company_reads_daily_cap",
    "cv_drafts_daily_cap",
    "cv_drafts_per_opportunity_cap",
    "letter_drafts_daily_cap",
    "letter_drafts_per_opportunity_cap",
    "ai_daily_limit_reached",
    "ai_budget_reached",
    "ai_unavailable",
    "profile_needed",
    "stage_too_early",
    "no_approved_cv",
    "company_unknown",
];

/// When the CV's approval of `opp` can still be undone while its letter is
/// queued: the letter waits until then.
async fn letter_waits_until(
    db: &PgPool,
    opp: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    let approved: Option<Option<DateTime<Utc>>> = sqlx::query_scalar(
        "SELECT d.approved_at
           FROM pipeline.preparations p
           JOIN pipeline.cv_drafts d
             ON d.opportunity_id = p.opportunity_id AND d.status = 'approved'
          WHERE p.opportunity_id = $1 AND p.state = 'queued' AND p.step = 'letter'
          ORDER BY d.approved_at DESC NULLS LAST LIMIT 1",
    )
    .bind(opp)
    .fetch_optional(db)
    .await?;
    Ok(approved
        .flatten()
        .map(|at| at + cv::UNDO_WINDOW)
        .filter(|until| *until > now))
}

/// Run `opp`'s queued steps.
pub(crate) async fn run(deps: &PipelineDeps, db: &PgPool, opp: Uuid) -> Result<()> {
    loop {
        // An approval undone and made again leaves the first approval's job
        // behind: the letter never starts while Undo is offered, and runs
        // once the newest approval's moment is over.
        if let Some(until) = letter_waits_until(db, opp, Utc::now()).await? {
            let mut conn = db.acquire().await?;
            jobs::enqueue_at(
                &mut conn,
                &Prepare { opportunity: opp },
                until + TimeDelta::seconds(1),
            )
            .await?;
            tracing::info!(opportunity = %opp, "a letter waits while its CV's approval can be undone");
            return Ok(());
        }
        let claimed: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
            "UPDATE pipeline.preparations SET state = 'running', updated_at = now()
              WHERE opportunity_id = $1 AND state = 'queued'
          RETURNING account_id, step, locale",
        )
        .bind(opp)
        .fetch_optional(db)
        .await?;
        let Some((account, step, locale)) = claimed else {
            return Ok(());
        };
        let step = PreparationStep::parse(&step);
        let now = Utc::now();
        let outcome = match refusal(deps, db, account, opp).await? {
            Some(code) => Err(code),
            None => run_step(deps, db, account, opp, step, locale.as_deref(), now)
                .await
                .map_err(|e| {
                    tracing::info!(
                        error = e.code(),
                        step = step.as_str(),
                        "a preparation step stopped"
                    );
                    e.code()
                }),
        };
        let (state, next_step, waiting_on, reason) = match outcome {
            Ok(Next::Run(next)) => (PreparationState::Queued, next, None, None),
            Ok(Next::Wait(on)) => (PreparationState::Waiting, step, Some(on), None),
            Ok(Next::Done) => (PreparationState::Done, step, None, None),
            Err(code)
                if STOPS.contains(&code) || code == "not_shortlisted" || code == "find_only" =>
            {
                (PreparationState::Stopped, step, None, Some(code))
            }
            Err(code) => (PreparationState::Failed, step, None, Some(code)),
        };
        // Stop (or moving the job) while it ran wins over what it came to.
        sqlx::query(
            "UPDATE pipeline.preparations
                SET state = $2, step = $3, waiting_on = $4, reason = $5, updated_at = $6
              WHERE opportunity_id = $1 AND state = 'running'",
        )
        .bind(opp)
        .bind(state.as_str())
        .bind(next_step.as_str())
        .bind(waiting_on.map(WaitingOn::as_str))
        .bind(reason)
        .bind(Utc::now())
        .execute(db)
        .await?;
        // Ids and kinds only.
        tracing::info!(
            opportunity = %opp,
            step = step.as_str(),
            state = state.as_str(),
            "a preparation step ran"
        );
    }
}

/// Note a step done by itself.
async fn record(db: &PgPool, opp: Uuid, step: &str, eur: f64, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "UPDATE pipeline.preparations
            SET done = done || jsonb_build_array(jsonb_build_object('step', $2::text, 'at', $3::timestamptz, 'eur', $4::float8))
          WHERE opportunity_id = $1",
    )
    .bind(opp)
    .bind(step)
    .bind(now)
    .bind(eur)
    .execute(db)
    .await?;
    Ok(())
}

/// Run one step; what comes next.
async fn run_step(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    opp: Uuid,
    step: PreparationStep,
    locale: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Next> {
    match step {
        PreparationStep::Read => {
            let had_site = brief::site_known(db, account, opp).await?;
            let prepared =
                brief::read_and_compare(deps, db, account, opp, locale, false, now).await?;
            if !prepared.site {
                return Ok(Next::Wait(WaitingOn::Site));
            }
            if !had_site {
                record(db, opp, "site", 0.0, now).await?;
            }
            if prepared.read {
                record(db, opp, "read", brief::COMPANY_READ_EUR, now).await?;
            }
            if prepared.compared {
                record(db, opp, "compare", brief::ANALYSIS_EUR, now).await?;
            }
            after_read(deps, db, account, opp, now).await
        }
        PreparationStep::Draft => {
            match after_read(deps, db, account, opp, now).await? {
                Next::Run(PreparationStep::Draft) => {}
                other => return Ok(other),
            }
            cv::draft(deps, db, account, opp, None, now).await?;
            record(db, opp, "draft", cv::DRAFT_EUR, now).await?;
            Ok(Next::Wait(WaitingOn::CvApproval))
        }
        PreparationStep::Letter => {
            match after_read(deps, db, account, opp, now).await? {
                Next::Run(PreparationStep::Letter) => {}
                other => return Ok(other),
            }
            letter::write(
                deps,
                db,
                account,
                opp,
                None,
                letter::model::LetterSize::default(),
                now,
            )
            .await?;
            record(db, opp, "letter", letter::DRAFT_EUR, now).await?;
            Ok(Next::Wait(WaitingOn::LetterApproval))
        }
    }
}

// ---- endpoints ------------------------------------------------------------

/// The language of the questions.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StartQuery {
    /// A language tag such as `pt-PT`; English when left out.
    locale: Option<String>,
}

/// Prepare this job by itself, again.
///
/// Queues the preparation from its first step (each step that is already done
/// is skipped without a model call): after Stop, after a cap or a limit
/// stopped it, or after a step failed (Try again). Needs the job at
/// Shortlisted or later, a model, and an Autonomy goal other than "Find
/// only". Runs in the background; the brief says where it is.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/prepare/start",
    tag = "pipeline",
    operation_id = "pipeline.prepare_start",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity."), StartQuery),
    responses(
        (status = 200, description = "The brief, with the preparation queued.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`stage_too_early`; `find_only`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
    Query(query): Query<StartQuery>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let now = state.clock.now();
    match refusal(&deps, &state.db, account, id).await? {
        None => {}
        Some("not_found") => return Err(Error::NotFound),
        Some("not_shortlisted") => {
            return Err(Error::Conflict(
                "stage_too_early",
                "Shortlist this job first: the app prepares the jobs you are going after."
                    .to_owned(),
            ));
        }
        Some("find_only") => {
            return Err(Error::Conflict(
                "find_only",
                "Your goals say the app only finds jobs. Change that in your goals first."
                    .to_owned(),
            ));
        }
        Some(_) => return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE)),
    }
    let mut tx = state.db.begin().await?;
    queue(
        &mut tx,
        account,
        id,
        PreparationStep::Read,
        query.locale.as_deref(),
        now,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        brief::brief_of(&deps, &state.db, account, id, now).await?,
    ))
}

/// Stop preparing this job by itself.
///
/// What is still to run does not run; what was made stays (a draft is still
/// there to approve or discard). "Prepare by itself" starts it again.
#[utoipa::path(
    post,
    path = "/api/v1/opportunities/{id}/prepare/stop",
    tag = "pipeline",
    operation_id = "pipeline.prepare_stop",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    responses(
        (status = 200, description = "The brief, with the preparation stopped.", body = Brief),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn stop(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<Brief>> {
    let account = principal.account_id;
    let now = state.clock.now();
    // Not found for another person's opportunity.
    let brief = brief::brief_of(&deps, &state.db, account, id, now).await?;
    let mut tx = state.db.begin().await?;
    cancel(&mut tx, brief.opportunity_id, "stopped", now).await?;
    tx.commit().await?;
    Ok(Json(
        brief::brief_of(&deps, &state.db, account, id, now).await?,
    ))
}
