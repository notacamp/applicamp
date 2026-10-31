//! `/api/v1/runs`: the Agents page. A person's runs, starting a check,
//! Stop, and what a run did (its events), live over SSE.
//!
//! Another account's run is not found.

#[cfg(test)]
mod tests;

use std::{convert::Infallible, time::Duration};

use applicamp_kernel::{AppState, Error, Problem, Result, agent::Outcome, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse as _, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use chrono::{DateTime, Utc};
use futures::StreamExt as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    RunsDeps,
    store::{self, Layer, RunEvent, RunRow, RunStatus},
};

/// Runs one account may have waiting at once.
pub const MAX_QUEUED: i64 = 3;
/// Events one page holds.
pub const PAGE: i64 = 500;
/// How often the live stream looks for new events.
const STREAM_POLL: Duration = Duration::from_secs(1);

/// A run, with its facts.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Run {
    pub id: Uuid,
    /// Its kind (`runs.probe`).
    pub kind: String,
    pub status: RunStatus,
    /// How it ended, once it has.
    pub outcome: Option<Outcome>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// When it is ended if still going.
    pub deadline: Option<DateTime<Utc>>,
    /// Its sandbox's name at the runtime.
    pub sandbox: Option<String>,
    /// The image its agent ran in.
    pub image: Option<String>,
    /// Its network policy (`policies/probe.yaml`).
    pub policy: Option<String>,
    /// The model that answered it, if it asked one.
    pub model: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// What its model calls cost, in euros.
    pub cost_eur: f64,
    /// From start to end (or to now, while it goes), in milliseconds.
    pub duration_ms: Option<i64>,
    /// Why it failed, as a code (`sandbox_create_failed`, `agent_exited`,
    /// `server_restarted`).
    pub error_kind: Option<String>,
    /// What its agent finished with.
    pub summary: Option<Value>,
    /// Someone pressed Stop and it has not ended yet.
    pub stopping: bool,
}

impl Run {
    pub(crate) fn from_row(row: RunRow, now: DateTime<Utc>) -> Self {
        let duration_ms = row.started_at.map(|start| {
            (row.finished_at.unwrap_or(now) - start)
                .num_milliseconds()
                .max(0)
        });
        let status = row.status();
        Self {
            outcome: row.outcome(),
            stopping: row.cancel_requested && status != RunStatus::Finished,
            status,
            id: row.id,
            kind: row.kind,
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            deadline: row.deadline,
            sandbox: row.sandbox,
            image: row.image,
            policy: row.policy,
            model: row.model,
            tokens_in: u32::try_from(row.tokens_in).unwrap_or_default(),
            tokens_out: u32::try_from(row.tokens_out).unwrap_or_default(),
            cost_eur: row.cost_eur,
            duration_ms,
            error_kind: row.error_kind,
            summary: row.summary,
        }
    }
}

/// A person's runs.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunList {
    /// Newest first, at most 100.
    pub runs: Vec<Run>,
    /// Whether runs can start on this instance (a sandbox runtime is
    /// configured).
    pub available: bool,
    /// The kinds a person may start here (`runs.probe`).
    pub startable: Vec<String>,
}

/// The signed-in person's runs.
///
/// Newest first, at most 100, with whether runs can start here and which
/// kinds a person may start.
#[utoipa::path(
    get,
    path = "/api/v1/runs",
    tag = "runs",
    operation_id = "runs.list",
    security(("session" = [])),
    responses(
        (status = 200, description = "The runs.", body = RunList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<RunsDeps>,
) -> Result<Json<RunList>> {
    let rows: Vec<RunRow> = sqlx::query_as(concat!(
        "SELECT ",
        store::run_columns!(),
        " FROM runs.runs WHERE account_id = $1 ORDER BY created_at DESC, id DESC LIMIT 100"
    ))
    .bind(principal.account_id)
    .fetch_all(&state.db)
    .await?;
    let now = state.clock.now();
    Ok(Json(RunList {
        runs: rows
            .into_iter()
            .map(|row| Run::from_row(row, now))
            .collect(),
        available: deps.runtime.is_some(),
        startable: deps
            .kinds
            .iter()
            .filter(|kind| kind.manual)
            .map(|kind| kind.name.to_owned())
            .collect(),
    }))
}

/// A run to start.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewRun {
    /// A kind a person may start (`runs.probe`: "Run a check").
    pub kind: String,
}

/// Start a run.
///
/// Queues a run of a kind a person may start (`runs.probe`, "Run a
/// check"). It starts when it is its turn: one run at a time per account,
/// three on the instance. At most three may wait.
#[utoipa::path(
    post,
    path = "/api/v1/runs",
    tag = "runs",
    operation_id = "runs.start",
    security(("session" = [])),
    request_body = NewRun,
    responses(
        (status = 201, description = "Queued.", body = Run),
        (status = 400, description = "`invalid_request`, `unknown_kind`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`too_many_runs`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`sandboxes_unavailable`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<RunsDeps>,
    body: std::result::Result<Json<NewRun>, JsonRejection>,
) -> Result<(StatusCode, Json<Run>)> {
    let Json(new) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let kind = deps
        .kinds
        .get(new.kind.trim())
        .filter(|kind| kind.manual)
        .ok_or_else(|| {
            Error::BadRequest(
                "unknown_kind",
                "That is not a run you can start.".to_owned(),
            )
        })?;
    if deps.runtime.is_none() {
        return Err(Error::Unavailable("sandboxes_unavailable"));
    }
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('runs.queue:' || $1::text))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let waiting: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runs.runs WHERE account_id = $1 AND status = 'queued'",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    if waiting >= MAX_QUEUED {
        return Err(Error::TooManyRequests(
            "too_many_runs",
            format!("{MAX_QUEUED} runs are already waiting. Try again once one has started."),
        ));
    }
    let id = store::queue(
        &mut tx,
        account,
        kind.name,
        None,
        &Value::Object(Default::default()),
        now,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(run = %id, kind = kind.name, "run queued");
    let row = store::fetch(&state.db, account, id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok((StatusCode::CREATED, Json(Run::from_row(row, now))))
}

/// A run.
///
/// One of the signed-in person's, with its facts. Anyone else's is not
/// found.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}",
    tag = "runs",
    operation_id = "runs.get",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The run.")),
    responses(
        (status = 200, description = "The run.", body = Run),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<Run>> {
    let row = store::fetch(&state.db, principal.account_id, id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(Json(Run::from_row(row, state.clock.now())))
}

/// Stop a run.
///
/// A waiting run ends `cancelled` at once; a going one is asked to stop:
/// its sandbox is removed and it ends `cancelled` with what it had used
/// (`stopping` until then).
#[utoipa::path(
    post,
    path = "/api/v1/runs/{id}/cancel",
    tag = "runs",
    operation_id = "runs.cancel",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The run.")),
    responses(
        (status = 202, description = "Stopping, or stopped.", body = Run),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`run_finished`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn cancel(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Run>)> {
    let account = principal.account_id;
    let row = store::fetch(&state.db, account, id)
        .await?
        .ok_or(Error::NotFound)?;
    let now = state.clock.now();
    match row.status() {
        RunStatus::Finished => {
            return Err(Error::Conflict(
                "run_finished",
                "This run has already ended.".to_owned(),
            ));
        }
        RunStatus::Queued => {
            // Unless it was claimed meanwhile; then it is asked to stop.
            let ended: Option<Uuid> = sqlx::query_scalar(
                "UPDATE runs.runs SET cancel_requested = true WHERE id = $1 AND status = 'queued'
              RETURNING id",
            )
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
            if ended.is_some() {
                store::finish(&state.db, &state.events, id, Outcome::Cancelled, None, now).await?;
            } else {
                request_stop(&state, id).await?;
            }
        }
        RunStatus::Starting | RunStatus::Running => request_stop(&state, id).await?,
    }
    tracing::info!(run = %id, "run stop asked");
    let row = store::fetch(&state.db, account, id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok((StatusCode::ACCEPTED, Json(Run::from_row(row, now))))
}

async fn request_stop(state: &AppState, id: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE runs.runs SET cancel_requested = true WHERE id = $1 AND status <> 'finished'",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Which events to list.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct EventsQuery {
    /// Only those after this number.
    pub after: Option<i64>,
    /// Only the agent's (`agent`) or the sandbox's (`sandbox`).
    pub layer: Option<Layer>,
}

/// A page of a run's events.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunEvents {
    /// Oldest first, at most 500; ask again with `after` the last `seq`.
    pub events: Vec<RunEvent>,
}

/// What a run did.
///
/// Its events, oldest first: the agent's own (`agent`: what it's doing) and
/// the sandbox's allow and deny lines (`sandbox`). Kept 30 days after the
/// run ends.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/events",
    tag = "runs",
    operation_id = "runs.events",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The run."), EventsQuery),
    responses(
        (status = 200, description = "The events.", body = RunEvents),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn events(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
    query: std::result::Result<Query<EventsQuery>, QueryRejection>,
) -> Result<Json<RunEvents>> {
    let Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    store::fetch(&state.db, principal.account_id, id)
        .await?
        .ok_or(Error::NotFound)?;
    let events =
        store::events_after(&state.db, id, query.after.unwrap_or(0), query.layer, PAGE).await?;
    Ok(Json(RunEvents { events }))
}

/// What a run does, live.
///
/// Server-sent events: each event is `event: run_event`, `id: <seq>`,
/// `data:` a `RunEvent` as JSON. Once the run has ended and every event was
/// sent, one `event: end` with the `Run` as `data`, and the stream closes.
/// Resumes after `Last-Event-ID` (or `?after=`).
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/events/stream",
    tag = "runs",
    operation_id = "runs.stream",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The run."), EventsQuery,
        ("Last-Event-ID" = Option<i64>, Header, description = "The last event seen.")),
    responses(
        (status = 200, description = "The live events.", content_type = "text/event-stream",
            body = String),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn stream(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    query: std::result::Result<Query<EventsQuery>, QueryRejection>,
) -> Result<Response> {
    let Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    store::fetch(&state.db, principal.account_id, id)
        .await?
        .ok_or(Error::NotFound)?;
    let after = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .or(query.after)
        .unwrap_or(0);
    let feed = Feed {
        state,
        account: principal.account_id,
        run: id,
        layer: query.layer,
        after,
        first: true,
        done: false,
    };
    let frames = futures::stream::unfold(feed, Feed::next)
        .flat_map(futures::stream::iter)
        .map(Ok::<_, Infallible>)
        .boxed();
    Ok(Sse::new(frames)
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// The live stream's position.
struct Feed {
    state: AppState,
    account: Uuid,
    run: Uuid,
    layer: Option<Layer>,
    after: i64,
    first: bool,
    done: bool,
}

impl Feed {
    /// The next frames: new events, or the end once the run is over.
    async fn next(mut self) -> Option<(Vec<Event>, Self)> {
        if self.done {
            return None;
        }
        loop {
            if !self.first {
                tokio::time::sleep(STREAM_POLL).await;
            }
            self.first = false;
            // Read the run first: once it is over, every event is in.
            let Ok(Some(row)) = store::fetch(&self.state.db, self.account, self.run).await else {
                return None;
            };
            let Ok(events) =
                store::events_after(&self.state.db, self.run, self.after, self.layer, PAGE).await
            else {
                return None;
            };
            if let Some(last) = events.last() {
                self.after = last.seq;
                let frames = events
                    .iter()
                    .filter_map(|event| {
                        Event::default()
                            .event("run_event")
                            .id(event.seq.to_string())
                            .json_data(event)
                            .ok()
                    })
                    .collect();
                // More may be waiting: read again without a pause.
                self.first = true;
                return Some((frames, self));
            }
            if row.status() == RunStatus::Finished {
                self.done = true;
                let run = Run::from_row(row, self.state.clock.now());
                let end = Event::default().event("end").json_data(&run).ok();
                return Some((end.into_iter().collect(), self));
            }
        }
    }
}
