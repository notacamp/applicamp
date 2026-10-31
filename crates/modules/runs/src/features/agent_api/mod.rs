//! `/api/agent/v1/…`: the agent API, the only way a sandbox reaches the
//! server (ADR 0003, 0009).
//!
//! Every call carries its run's token as a bearer (`run_token`) and no
//! cookie. The token names its run (so its account and subject) and the
//! kind's tools with their caps; it works only while its run is going and
//! the agent has not finished, and never past the deadline plus five
//! minutes. Anything else is not found. A run may make ten calls a second.
//! The door is outside the CSRF check (no browser sends a run token).
//!
//! - `GET context`: the run, its kind, inputs, subject, limits and tools.
//! - `POST events`: the agent's own event lines (numbered by the agent, so a
//!   line also read from its output is kept once).
//! - `POST tools/{name}`: one call of one of the kind's tools, checked
//!   against its schema and its cap, handled by the module that owns it.
//! - `POST complete`: one completion of the `complete` role through
//!   `applicamp-ai`, for the run's account, within the run's token budget.
//! - `POST finish`: the agent's outcome and summary (checked against the
//!   kind's schema). The server ends the run once the sandbox is gone.
//!
//! Content from the web reaches the server here as data: it is stored and
//! shown, never obeyed.

#[cfg(test)]
mod tests;

use applicamp_ai::{Caller, ChatRequest, Message, MessageRole, ResponseFormat};
use applicamp_kernel::{
    AppState, Error, Problem, Result,
    agent::{Outcome, RunKind, ToolClass, ToolContext},
    crypto,
};
use axum::{
    Extension, Json,
    extract::{FromRequestParts, Path, rejection::JsonRejection},
    http::{StatusCode, header, request::Parts},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    RunsDeps, schema,
    store::{self, Layer, Level, MAX_MESSAGE, MAX_NAME, NewEvent},
};

/// Events one call may carry.
pub const MAX_EVENTS: usize = 50;
/// Largest `data` of an event, as JSON.
pub const MAX_DATA: usize = 8 * 1024;
/// Longest answer one completion may ask for.
pub const MAX_COMPLETION_TOKENS: u32 = 16_000;
/// Messages one completion may carry.
pub const MAX_MESSAGES: usize = 50;

/// The problem code of a call without a run token.
pub const RUN_TOKEN_REQUIRED: &str = "run_token_required";

/// The run behind an agent API call: its token checked.
#[derive(Debug, Clone)]
pub struct RunCaller {
    pub run: Uuid,
    pub account: Uuid,
    pub kind: String,
    pub subject: Option<Uuid>,
    pub inputs: Value,
    pub deadline: Option<DateTime<Utc>>,
    /// `{ "<tool>": <cap> }`, as the token says.
    pub tools: Value,
}

#[derive(sqlx::FromRow)]
struct CallerRow {
    run_id: Uuid,
    account_id: Uuid,
    kind: String,
    subject_id: Option<Uuid>,
    inputs: Value,
    deadline: Option<DateTime<Utc>>,
    tools: Value,
}

impl FromRequestParts<AppState> for RunCaller {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if parts.headers.contains_key(header::COOKIE) {
            return Err(Error::BadRequest(
                "cookies_refused",
                "The agent API takes a run token, never a cookie.".to_owned(),
            ));
        }
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or(Error::Unauthorized(RUN_TOKEN_REQUIRED))?;
        let row: Option<CallerRow> = sqlx::query_as(
            "SELECT t.run_id, r.account_id, r.kind, r.subject_id, r.inputs, r.deadline, t.tools
               FROM runs.tokens t JOIN runs.runs r ON r.id = t.run_id
              WHERE t.hash = $1 AND t.expires_at > $2
                AND r.status = 'running' AND r.outcome IS NULL",
        )
        .bind(crypto::hash(token))
        .bind(state.clock.now())
        .fetch_optional(&state.db)
        .await?;
        let row = row.ok_or(Error::NotFound)?;
        if let Some(deps) = parts.extensions.get::<RunsDeps>()
            && !deps.limiter.allow(row.run_id)
        {
            return Err(Error::TooManyRequests(
                "rate_limited",
                "At most ten calls a second.".to_owned(),
            ));
        }
        Ok(Self {
            run: row.run_id,
            account: row.account_id,
            kind: row.kind,
            subject: row.subject_id,
            inputs: row.inputs,
            deadline: row.deadline,
            tools: row.tools,
        })
    }
}

impl RunCaller {
    fn kind<'a>(&self, deps: &'a RunsDeps) -> Result<&'a RunKind> {
        deps.kinds.get(&self.kind).ok_or(Error::NotFound)
    }
}

/// The limits of the run.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RunLimits {
    pub max_steps: u32,
    /// Seconds from start to the deadline.
    pub wall_clock_secs: u64,
    /// Model tokens the run may use.
    pub max_tokens: u32,
    pub deadline: Option<DateTime<Utc>>,
}

/// A tool the agent may call.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentTool {
    pub name: String,
    pub class: ToolClass,
    /// The JSON schema its arguments match.
    pub schema: Value,
    /// Calls the run may make.
    pub cap: u32,
}

/// What the agent works on.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentContext {
    pub run: Uuid,
    pub kind: String,
    /// Ids and settings, never personal data.
    pub inputs: Value,
    pub subject: Option<Uuid>,
    pub limits: RunLimits,
    pub tools: Vec<AgentTool>,
}

/// The run's context.
///
/// The run, its kind, inputs, subject, limits, and the tools the agent may
/// call with their schemas and caps.
#[utoipa::path(
    get,
    path = "/api/agent/v1/context",
    tag = "runs",
    operation_id = "runs.agent_context",
    security(("run_token" = [])),
    responses(
        (status = 200, description = "The context.", body = AgentContext),
        (status = 400, description = "`cookies_refused`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`run_token_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such run going", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`rate_limited`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn context(
    caller: RunCaller,
    Extension(deps): Extension<RunsDeps>,
) -> Result<Json<AgentContext>> {
    let kind = caller.kind(&deps)?;
    Ok(Json(AgentContext {
        run: caller.run,
        kind: caller.kind.clone(),
        inputs: caller.inputs.clone(),
        subject: caller.subject,
        limits: RunLimits {
            max_steps: kind.limits.max_steps,
            wall_clock_secs: kind.limits.wall_clock.as_secs(),
            max_tokens: kind.limits.max_tokens,
            deadline: caller.deadline,
        },
        tools: kind
            .tools
            .iter()
            .filter(|tool| caller.tools.get(tool.name).is_some())
            .map(|tool| AgentTool {
                name: tool.name.to_owned(),
                class: tool.class,
                schema: tool.schema.clone(),
                cap: tool.cap,
            })
            .collect(),
    }))
}

/// One event line of the agent's.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEvent {
    /// The agent's own number for it, from 1; the same line sent twice is
    /// kept once.
    pub seq: i64,
    pub level: Level,
    /// At most 2 000 characters.
    pub message: String,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    /// At most 8 KB as JSON.
    #[serde(default)]
    pub data: Option<Value>,
}

/// What was recorded.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Recorded {
    /// New lines (a line already recorded is not counted).
    pub recorded: usize,
}

fn invalid_event(detail: impl Into<String>) -> Error {
    Error::BadRequest("invalid_event", detail.into())
}

/// Record the agent's events.
///
/// At most 50 a call, each numbered by the agent, its message at most
/// 2 000 characters, its data at most 8 KB.
#[utoipa::path(
    post,
    path = "/api/agent/v1/events",
    tag = "runs",
    operation_id = "runs.agent_events",
    security(("run_token" = [])),
    request_body = Vec<AgentEvent>,
    responses(
        (status = 202, description = "Recorded.", body = Recorded),
        (status = 400, description = "`invalid_request`, `invalid_event`, `cookies_refused`",
            body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`run_token_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`rate_limited`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn events(
    axum::extract::State(state): axum::extract::State<AppState>,
    caller: RunCaller,
    body: std::result::Result<Json<Vec<AgentEvent>>, JsonRejection>,
) -> Result<(StatusCode, Json<Recorded>)> {
    let Json(events) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    if events.len() > MAX_EVENTS {
        return Err(invalid_event(format!(
            "At most {MAX_EVENTS} events a call."
        )));
    }
    for event in &events {
        if event.seq < 1 {
            return Err(invalid_event("`seq` starts at 1."));
        }
        if event.message.chars().count() > MAX_MESSAGE {
            return Err(invalid_event(format!(
                "A message is at most {MAX_MESSAGE} characters."
            )));
        }
        if [&event.operation, &event.tool_name]
            .into_iter()
            .flatten()
            .any(|name| name.chars().count() > MAX_NAME)
        {
            return Err(invalid_event(format!(
                "A name is at most {MAX_NAME} characters."
            )));
        }
        if event
            .data
            .as_ref()
            .is_some_and(|data| data.to_string().len() > MAX_DATA)
        {
            return Err(invalid_event("`data` is at most 8 KB."));
        }
    }
    let now = state.clock.now();
    let mut recorded = 0;
    for event in events {
        let new = NewEvent {
            layer: Layer::Agent,
            level: event.level,
            agent_seq: Some(event.seq),
            operation: event.operation,
            tool_name: event.tool_name,
            message: event.message,
            data: event.data,
        };
        if store::insert_event(&state.db, caller.run, now, &new).await? {
            recorded += 1;
        }
    }
    Ok((StatusCode::ACCEPTED, Json(Recorded { recorded })))
}

/// One tool call.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    /// The arguments, matching the tool's schema.
    #[serde(default)]
    pub args: Value,
}

/// What a tool answered.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ToolResult {
    pub result: Value,
}

/// Call a tool.
///
/// One call of one of the kind's tools. A tool the kind does not have is
/// not found; arguments that do not match its schema are refused
/// (`invalid_arguments`, saying where) and do not count; past its cap the
/// call is refused (`tool_cap_reached`).
#[utoipa::path(
    post,
    path = "/api/agent/v1/tools/{name}",
    tag = "runs",
    operation_id = "runs.agent_tool",
    security(("run_token" = [])),
    params(("name" = String, Path, description = "The tool.")),
    request_body = ToolCall,
    responses(
        (status = 200, description = "Its answer.", body = ToolResult),
        (status = 400, description = "`invalid_request`, `invalid_arguments`, or the tool's \
            own refusal", body = Problem, content_type = "application/problem+json"),
        (status = 401, description = "`run_token_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`tool_cap_reached`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`rate_limited`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn tool(
    axum::extract::State(state): axum::extract::State<AppState>,
    caller: RunCaller,
    Extension(deps): Extension<RunsDeps>,
    Path(name): Path<String>,
    body: std::result::Result<Json<ToolCall>, JsonRejection>,
) -> Result<Json<ToolResult>> {
    let kind = caller.kind(&deps)?;
    let cap = caller.tools.get(&name).and_then(Value::as_u64);
    let (Some(tool), Some(cap)) = (kind.tool(&name), cap) else {
        return Err(Error::NotFound);
    };
    let Json(call) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    schema::check(&tool.schema, &call.args)
        .map_err(|why| Error::BadRequest("invalid_arguments", why))?;
    let counted: Option<i64> = sqlx::query_scalar(
        "UPDATE runs.tokens
            SET calls = jsonb_set(calls, ARRAY[$2::text],
                                  to_jsonb(coalesce((calls ->> $2)::bigint, 0) + 1))
          WHERE run_id = $1 AND coalesce((calls ->> $2)::bigint, 0) < $3
      RETURNING (calls ->> $2)::bigint",
    )
    .bind(caller.run)
    .bind(&name)
    .bind(i64::try_from(cap).unwrap_or(i64::MAX))
    .fetch_optional(&state.db)
    .await?;
    if counted.is_none() {
        tracing::info!(run = %caller.run, tool = %name, "a tool call past its cap");
        return Err(Error::Conflict(
            "tool_cap_reached",
            format!("`{name}` may be called {cap} time(s) in a run."),
        ));
    }
    let ctx = ToolContext {
        db: &state.db,
        run: caller.run,
        account: caller.account,
        subject: caller.subject,
        now: state.clock.now(),
    };
    let result = tool.handler.call(ctx, call.args).await?;
    tracing::info!(run = %caller.run, tool = %name, "tool called");
    Ok(Json(ToolResult { result }))
}

/// Who said a message.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CompletionRole {
    System,
    User,
    Assistant,
}

/// A message of a completion.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompletionMessage {
    pub role: CompletionRole,
    pub content: String,
}

/// The shape the answer must take.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompletionFormat {
    /// Any JSON object.
    JsonObject,
    /// JSON matching `schema`, named `name`.
    JsonSchema { name: String, schema: Value },
}

/// One completion.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    /// At most 50.
    pub messages: Vec<CompletionMessage>,
    #[serde(default)]
    pub response_format: Option<CompletionFormat>,
    /// Longest answer, at most 16 000.
    pub max_tokens: u32,
}

/// A model's answer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompletionAnswer {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

async fn record(state: &AppState, run: Uuid, level: Level, message: String) {
    let event = NewEvent {
        layer: Layer::Agent,
        level,
        agent_seq: None,
        operation: Some("complete".to_owned()),
        tool_name: None,
        message,
        data: None,
    };
    if let Err(e) = store::insert_event(&state.db, run, state.clock.now(), &event).await {
        tracing::warn!(run = %run, error = %e, "could not record a run event");
    }
}

/// Ask the model.
///
/// One completion of the `complete` role through the AI gateway, for the
/// run's account: refused when the account reached its daily tokens or a
/// spending limit (`ai_daily_limit_reached`, `ai_budget_reached`), and
/// when the run's tokens with this answer's worst case would pass its
/// kind's budget (`run_budget_reached`). The run's tokens and cost grow by
/// what it used.
#[utoipa::path(
    post,
    path = "/api/agent/v1/complete",
    tag = "runs",
    operation_id = "runs.agent_complete",
    security(("run_token" = [])),
    request_body = Completion,
    responses(
        (status = 200, description = "The answer.", body = CompletionAnswer),
        (status = 400, description = "`invalid_request`, `invalid_completion`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`run_token_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`run_budget_reached`, `rate_limited`", body = Problem,
            content_type = "application/problem+json"),
        (status = 503, description = "`ai_unavailable`, `ai_daily_limit_reached`, \
            `ai_budget_reached`, `ai_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn complete(
    axum::extract::State(state): axum::extract::State<AppState>,
    caller: RunCaller,
    Extension(deps): Extension<RunsDeps>,
    body: std::result::Result<Json<Completion>, JsonRejection>,
) -> Result<Json<CompletionAnswer>> {
    let kind = caller.kind(&deps)?;
    let Json(completion) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    if completion.messages.is_empty()
        || completion.messages.len() > MAX_MESSAGES
        || completion.max_tokens == 0
        || completion.max_tokens > MAX_COMPLETION_TOKENS
    {
        return Err(Error::BadRequest(
            "invalid_completion",
            format!(
                "1 to {MAX_MESSAGES} messages, and `max_tokens` from 1 to {MAX_COMPLETION_TOKENS}."
            ),
        ));
    }
    let used: i64 = sqlx::query_scalar(
        "SELECT tokens_in::bigint + tokens_out::bigint FROM runs.runs WHERE id = $1",
    )
    .bind(caller.run)
    .fetch_one(&state.db)
    .await?;
    if used + i64::from(completion.max_tokens) > i64::from(kind.limits.max_tokens) {
        record(
            &state,
            caller.run,
            Level::Error,
            "Model call refused: the run's token budget is reached.".to_owned(),
        )
        .await;
        return Err(Error::TooManyRequests(
            "run_budget_reached",
            format!("This run may use {} model tokens.", kind.limits.max_tokens),
        ));
    }
    let Some(ai) = &deps.ai else {
        return Err(Error::Unavailable(applicamp_ai::AI_UNAVAILABLE));
    };
    let mut request = ChatRequest::new(
        completion
            .messages
            .into_iter()
            .map(|message| Message {
                role: match message.role {
                    CompletionRole::System => MessageRole::System,
                    CompletionRole::User => MessageRole::User,
                    CompletionRole::Assistant => MessageRole::Assistant,
                },
                content: message.content,
            })
            .collect(),
    );
    request.max_tokens = Some(completion.max_tokens);
    request.response_format = completion.response_format.map(|format| match format {
        CompletionFormat::JsonObject => ResponseFormat::JsonObject,
        CompletionFormat::JsonSchema { name, schema } => ResponseFormat::json_schema(name, schema),
    });
    let caller_ai = Caller {
        db: &state.db,
        account: caller.account,
        now: state.clock.now(),
    };
    let answer = match ai.complete(caller_ai, request).await {
        Ok(answer) => answer,
        Err(err) => {
            let err = Error::from(err);
            record(
                &state,
                caller.run,
                Level::Error,
                format!("Model call refused: {}.", err.code()),
            )
            .await;
            return Err(err);
        }
    };
    let usage = answer.usage.unwrap_or_default();
    let cost_eur = answer.cost_usd.unwrap_or_default() * ai.usd_to_eur();
    sqlx::query(
        "UPDATE runs.runs
            SET tokens_in = tokens_in + $2, tokens_out = tokens_out + $3,
                cost_eur = cost_eur + $4, model = $5
          WHERE id = $1",
    )
    .bind(caller.run)
    .bind(i32::try_from(usage.prompt_tokens).unwrap_or(i32::MAX))
    .bind(i32::try_from(usage.completion_tokens).unwrap_or(i32::MAX))
    .bind(cost_eur)
    .bind(&answer.model)
    .execute(&state.db)
    .await?;
    Ok(Json(CompletionAnswer {
        content: answer.content,
        model: answer.model,
        finish_reason: answer.finish_reason,
        tokens_in: usage.prompt_tokens,
        tokens_out: usage.completion_tokens,
    }))
}

/// How the agent says it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutcome {
    Succeeded,
    NothingNew,
    NeedsYou,
    Failed,
}

impl From<AgentOutcome> for Outcome {
    fn from(outcome: AgentOutcome) -> Self {
        match outcome {
            AgentOutcome::Succeeded => Self::Succeeded,
            AgentOutcome::NothingNew => Self::NothingNew,
            AgentOutcome::NeedsYou => Self::NeedsYou,
            AgentOutcome::Failed => Self::Failed,
        }
    }
}

/// The agent's end.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub outcome: AgentOutcome,
    /// Matching the kind's output schema.
    pub summary: Value,
}

/// Finish.
///
/// The agent's outcome and summary (checked against the kind's output
/// schema: `invalid_summary`). The token stops working; the server ends the
/// run once the sandbox is gone.
#[utoipa::path(
    post,
    path = "/api/agent/v1/finish",
    tag = "runs",
    operation_id = "runs.agent_finish",
    security(("run_token" = [])),
    request_body = Finish,
    responses(
        (status = 202, description = "Recorded."),
        (status = 400, description = "`invalid_request`, `invalid_summary`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`run_token_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 429, description = "`rate_limited`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn finish(
    axum::extract::State(state): axum::extract::State<AppState>,
    caller: RunCaller,
    Extension(deps): Extension<RunsDeps>,
    body: std::result::Result<Json<Finish>, JsonRejection>,
) -> Result<StatusCode> {
    let kind = caller.kind(&deps)?;
    let Json(finish) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    schema::check(&kind.output, &finish.summary)
        .map_err(|why| Error::BadRequest("invalid_summary", why))?;
    let outcome = Outcome::from(finish.outcome);
    debug_assert!(crate::agent_outcomes().contains(&outcome));
    let done = sqlx::query(
        "UPDATE runs.runs SET outcome = $2, summary = $3
          WHERE id = $1 AND status = 'running' AND outcome IS NULL",
    )
    .bind(caller.run)
    .bind(outcome.as_str())
    .bind(&finish.summary)
    .execute(&state.db)
    .await?;
    if done.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    tracing::info!(run = %caller.run, outcome = outcome.as_str(), "the agent finished");
    Ok(StatusCode::ACCEPTED)
}
