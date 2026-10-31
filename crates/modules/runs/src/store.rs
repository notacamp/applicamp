//! The `runs` schema: runs, their events and their tokens.

use std::collections::HashSet;

use applicamp_kernel::{
    EventBus,
    agent::{Outcome, RunFinished},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

/// Longest event message, in characters.
pub const MAX_MESSAGE: usize = 2000;
/// Longest operation or tool name on an event, in characters.
pub const MAX_NAME: usize = 100;

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Waiting its turn.
    Queued,
    /// Its sandbox is being made.
    Starting,
    /// Its agent is working.
    Running,
    /// Over; see its outcome.
    Finished,
}

impl RunStatus {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Finished => "finished",
        }
    }

    /// The status stored as `raw`.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "starting" => Self::Starting,
            "running" => Self::Running,
            "finished" => Self::Finished,
            _ => Self::Queued,
        }
    }
}

/// Who wrote an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// The agent itself ("What it's doing").
    Agent,
    /// The sandbox's network proxy ("Sandbox log").
    Sandbox,
}

impl Layer {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Sandbox => "sandbox",
        }
    }

    fn parse(raw: &str) -> Self {
        if raw == "sandbox" {
            Self::Sandbox
        } else {
            Self::Agent
        }
    }
}

/// What kind of line an event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Something it did or saw.
    Info,
    /// A tool call.
    Tool,
    /// A model call.
    Model,
    /// Something refused (a host outside the policy, a call past its cap).
    Deny,
    /// A question for the person.
    Ask,
    /// Something that went wrong.
    Error,
}

impl Level {
    /// Every level.
    pub const ALL: [Self; 6] = [
        Self::Info,
        Self::Tool,
        Self::Model,
        Self::Deny,
        Self::Ask,
        Self::Error,
    ];

    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Tool => "tool",
            Self::Model => "model",
            Self::Deny => "deny",
            Self::Ask => "ask",
            Self::Error => "error",
        }
    }

    fn parse(raw: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|level| level.as_str() == raw)
            .unwrap_or(Self::Info)
    }
}

/// `text` cut to `max` characters.
pub fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A run, as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub kind: String,
    pub subject_id: Option<Uuid>,
    pub status: String,
    pub outcome: Option<String>,
    pub inputs: Value,
    pub deadline: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub sandbox: Option<String>,
    pub image: Option<String>,
    pub policy: Option<String>,
    pub model: Option<String>,
    pub tokens_in: i32,
    pub tokens_out: i32,
    pub cost_eur: f64,
    pub error_kind: Option<String>,
    pub summary: Option<Value>,
    pub cancel_requested: bool,
}

impl RunRow {
    /// Its status.
    pub fn status(&self) -> RunStatus {
        RunStatus::parse(&self.status)
    }

    /// Its outcome, when it has one.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome.as_deref().and_then(Outcome::parse)
    }
}

/// The columns a [`RunRow`] reads, as a literal (queries are static strings).
macro_rules! run_columns {
    () => {
        "id, account_id, kind, subject_id, status, outcome, inputs, deadline, created_at, \
         started_at, finished_at, sandbox, image, policy, model, tokens_in, tokens_out, cost_eur, \
         error_kind, summary, cancel_requested"
    };
}
pub(crate) use run_columns;

/// Queue a run in `conn`'s transaction. Its id.
pub async fn queue(
    conn: &mut PgConnection,
    account: Uuid,
    kind: &str,
    subject: Option<Uuid>,
    inputs: &Value,
    now: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO runs.runs (id, account_id, kind, subject_id, inputs, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(account)
    .bind(kind)
    .bind(subject)
    .bind(inputs)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(id)
}

/// Account `account`'s run `id`.
pub async fn fetch(db: &PgPool, account: Uuid, id: Uuid) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        run_columns!(),
        " FROM runs.runs WHERE id = $1 AND account_id = $2"
    ))
    .bind(id)
    .bind(account)
    .fetch_optional(db)
    .await
}

/// Run `id`, whoever's it is.
pub async fn fetch_any(db: &PgPool, id: Uuid) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        run_columns!(),
        " FROM runs.runs WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

/// Claim the oldest queued run that may start: none of its account's is
/// going and fewer than `max_runs` are on the instance. Claims are taken one
/// at a time across servers, so the limits hold.
pub async fn claim(
    db: &PgPool,
    instance: &str,
    max_runs: i64,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Option<RunRow>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('runs.claim'))")
        .execute(&mut *tx)
        .await?;
    let claimed: Option<RunRow> = sqlx::query_as(concat!(
        "UPDATE runs.runs
            SET status = 'starting', claimed_by = $1, claimed_until = $2, started_at = $3
          WHERE id = (SELECT r.id FROM runs.runs r
                       WHERE r.status = 'queued'
                         AND NOT EXISTS (SELECT 1 FROM runs.runs o
                                          WHERE o.account_id = r.account_id
                                            AND o.status IN ('starting', 'running'))
                         AND (SELECT count(*) FROM runs.runs l
                               WHERE l.status IN ('starting', 'running')) < $4
                       ORDER BY r.created_at, r.id
                       LIMIT 1
                         FOR UPDATE SKIP LOCKED)
      RETURNING ",
        run_columns!()
    ))
    .bind(instance)
    .bind(until)
    .bind(now)
    .bind(max_runs)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(claimed)
}

/// Keep `instance`'s claim on run `id` until `until`. Whether someone asked
/// it to stop; `None` when the claim is gone (the run was ended elsewhere).
pub async fn renew(
    db: &PgPool,
    id: Uuid,
    instance: &str,
    until: DateTime<Utc>,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE runs.runs SET claimed_until = $3
          WHERE id = $1 AND claimed_by = $2 AND status IN ('starting', 'running')
      RETURNING cancel_requested",
    )
    .bind(id)
    .bind(instance)
    .bind(until)
    .fetch_optional(db)
    .await
}

/// Run `id` starts: its deadline, image and policy, and its token (stored
/// as `hash`, with its kind's tools and their caps).
#[allow(clippy::too_many_arguments)]
pub async fn start(
    db: &PgPool,
    id: Uuid,
    deadline: DateTime<Utc>,
    image: &str,
    policy: &str,
    hash: &[u8],
    tools: &Value,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE runs.runs SET deadline = $2, image = $3, policy = $4 WHERE id = $1")
        .bind(id)
        .bind(deadline)
        .bind(image)
        .bind(policy)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO runs.tokens (run_id, hash, tools, expires_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (run_id) DO UPDATE SET hash = $2, tools = $3, calls = '{}'::jsonb,
                                            expires_at = $4",
    )
    .bind(id)
    .bind(hash)
    .bind(tools)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Run `id`'s sandbox `sandbox` exists and its agent works.
pub async fn running(db: &PgPool, id: Uuid, sandbox: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE runs.runs SET status = 'running', sandbox = $2
          WHERE id = $1 AND status = 'starting'",
    )
    .bind(id)
    .bind(sandbox)
    .execute(db)
    .await?;
    Ok(())
}

/// End run `id` with `outcome` (and `error_kind`), unless it already ended:
/// its token is deleted and `runs.finished` published in the same
/// transaction. Whether it ended now.
pub async fn finish(
    db: &PgPool,
    events: &EventBus,
    id: Uuid,
    outcome: Outcome,
    error_kind: Option<&str>,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let ended: Option<(Uuid, String, Option<Uuid>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "UPDATE runs.runs
            SET status = 'finished', outcome = $2, error_kind = coalesce($3, error_kind),
                finished_at = $4, claimed_until = NULL
          WHERE id = $1 AND status <> 'finished'
      RETURNING account_id, kind, subject_id, started_at",
    )
    .bind(id)
    .bind(outcome.as_str())
    .bind(error_kind)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((account, kind, subject, started_at)) = ended else {
        return Ok(false);
    };
    ended_run(&mut tx, events, id, account, kind.clone(), subject, outcome).await?;
    tx.commit().await?;
    let duration_ms = started_at.map_or(0, |at| (now - at).num_milliseconds());
    tracing::info!(run = %id, kind, outcome = outcome.as_str(), error_kind, duration_ms, "run finished");
    Ok(true)
}

async fn ended_run(
    conn: &mut PgConnection,
    events: &EventBus,
    run: Uuid,
    account: Uuid,
    kind: String,
    subject: Option<Uuid>,
    outcome: Outcome,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM runs.tokens WHERE run_id = $1")
        .bind(run)
        .execute(&mut *conn)
        .await?;
    events
        .publish(
            conn,
            &RunFinished {
                run,
                account,
                kind,
                subject,
                outcome,
            },
        )
        .await?;
    Ok(())
}

/// End every run whose server's claim lapsed (it died mid-run) as failed,
/// `server_restarted`. How many.
pub async fn reap(
    db: &PgPool,
    events: &EventBus,
    now: DateTime<Utc>,
) -> Result<usize, sqlx::Error> {
    let mut tx = db.begin().await?;
    let reaped: Vec<(Uuid, Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "UPDATE runs.runs
            SET status = 'finished', outcome = 'failed', error_kind = 'server_restarted',
                finished_at = $1, claimed_until = NULL
          WHERE status IN ('starting', 'running') AND claimed_until < $1
      RETURNING id, account_id, kind, subject_id",
    )
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    for (run, account, kind, subject) in &reaped {
        ended_run(
            &mut tx,
            events,
            *run,
            *account,
            kind.clone(),
            *subject,
            Outcome::Failed,
        )
        .await?;
        tracing::warn!(run = %run, kind, "run ended: its server stopped mid-run");
    }
    tx.commit().await?;
    Ok(reaped.len())
}

/// The runs that are going (their sandboxes are kept by a sweep).
pub async fn live(db: &PgPool) -> Result<HashSet<Uuid>, sqlx::Error> {
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM runs.runs WHERE status IN ('starting', 'running')")
            .fetch_all(db)
            .await?;
    Ok(ids.into_iter().collect())
}

/// An event to record.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub layer: Layer,
    pub level: Level,
    /// The agent's own number for it, when it gave one.
    pub agent_seq: Option<i64>,
    pub operation: Option<String>,
    pub tool_name: Option<String>,
    pub message: String,
    pub data: Option<Value>,
}

/// Record `event` for run `run` at `at`, under the next number. Whether it
/// was new (an agent event with a number already recorded is not).
pub async fn insert_event(
    db: &PgPool,
    run: Uuid,
    at: DateTime<Utc>,
    event: &NewEvent,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "WITH s AS (UPDATE runs.runs SET last_seq = last_seq + 1 WHERE id = $1 RETURNING last_seq)
         INSERT INTO runs.events
                (run_id, seq, agent_seq, at, layer, level, operation, tool_name, message, data)
         SELECT $1, s.last_seq, $2, $3, $4, $5, $6, $7, $8, $9 FROM s
         ON CONFLICT (run_id, agent_seq) DO NOTHING",
    )
    .bind(run)
    .bind(event.agent_seq)
    .bind(at)
    .bind(event.layer.as_str())
    .bind(event.level.as_str())
    .bind(event.operation.as_deref().map(|s| clip(s, MAX_NAME)))
    .bind(event.tool_name.as_deref().map(|s| clip(s, MAX_NAME)))
    .bind(clip(&event.message, MAX_MESSAGE))
    .bind(&event.data)
    .execute(db)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// A recorded event.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunEvent {
    /// Its number in the run: the live stream's `Last-Event-ID`.
    pub seq: i64,
    pub at: DateTime<Utc>,
    pub layer: Layer,
    pub level: Level,
    /// What it was (`NET:OPEN`, `context`), when it says.
    pub operation: Option<String>,
    /// The tool it is about, if any.
    pub tool_name: Option<String>,
    /// The line, at most 2 000 characters. Written by the agent or the
    /// sandbox: text to show, never markup.
    pub message: String,
    /// What the agent attached to it.
    pub data: Option<Value>,
}

#[derive(sqlx::FromRow)]
struct EventRow {
    seq: i64,
    at: DateTime<Utc>,
    layer: String,
    level: String,
    operation: Option<String>,
    tool_name: Option<String>,
    message: String,
    data: Option<Value>,
}

impl From<EventRow> for RunEvent {
    fn from(row: EventRow) -> Self {
        Self {
            seq: row.seq,
            at: row.at,
            layer: Layer::parse(&row.layer),
            level: Level::parse(&row.level),
            operation: row.operation,
            tool_name: row.tool_name,
            message: row.message,
            data: row.data,
        }
    }
}

/// Run `run`'s events after `after`, oldest first, at most `limit`; only
/// `layer`'s when given.
pub async fn events_after(
    db: &PgPool,
    run: Uuid,
    after: i64,
    layer: Option<Layer>,
    limit: i64,
) -> Result<Vec<RunEvent>, sqlx::Error> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT seq, at, layer, level, operation, tool_name, message, data FROM runs.events
          WHERE run_id = $1 AND seq > $2 AND ($3::text IS NULL OR layer = $3)
          ORDER BY seq
          LIMIT $4",
    )
    .bind(run)
    .bind(after)
    .bind(layer.map(Layer::as_str))
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(RunEvent::from).collect())
}

/// Delete the events of runs that finished before `before`; the runs and
/// their summaries stay. How many.
pub async fn prune_events(db: &PgPool, before: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "DELETE FROM runs.events e USING runs.runs r
          WHERE e.run_id = r.id AND r.status = 'finished' AND r.finished_at < $1",
    )
    .bind(before)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}
