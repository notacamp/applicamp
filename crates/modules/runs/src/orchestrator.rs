//! The orchestrator loop: claims queued runs and sees each one through.
//!
//! A run outlives the job queue's lease (60 s), so the queue is
//! `runs.runs` itself: the server claims a queued run within the limits
//! (one per account, three on the instance) and keeps its claim while the
//! run goes, renewing it every second. For each run it
//!
//! 1. mints the run's token (kept as its hash, with the kind's tools),
//! 2. creates the sandbox with the kind's image and policy, the run id, the
//!    token and the agent API's address,
//! 3. follows the sandbox's log into the run's events,
//! 4. waits for the agent to exit, the deadline, or Stop,
//! 5. **always** deletes the sandbox,
//! 6. records the outcome, deletes the token and publishes `runs.finished`.
//!
//! A server that dies stops renewing its claims: once one lapses the run
//! ends `failed` (`server_restarted`), and the sweep (at startup, then
//! every five minutes and after each such end) removes every Applicamp
//! sandbox whose run is not going.

use std::{sync::Arc, time::Duration};

use applicamp_kernel::{EventBus, agent::Outcome, crypto};
use chrono::{DateTime, TimeDelta, Utc};
use futures::StreamExt as _;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    RunsDeps,
    runtime::{Exit, LogLine, SandboxHandle, SandboxRuntime, SandboxSpec},
    store::{self, Layer, Level, NewEvent, RunRow},
};

/// How long a claim holds without being renewed.
pub const LEASE: TimeDelta = TimeDelta::seconds(30);
/// How often a run's claim is renewed and Stop looked for.
pub const TICK: Duration = Duration::from_secs(1);
/// How often the loop looks for queued runs when idle.
pub const POLL: Duration = Duration::from_secs(1);
/// How often left-over sandboxes are swept.
pub const SWEEP_EVERY: Duration = Duration::from_secs(5 * 60);
/// How long a token outlives its run's deadline.
pub const TOKEN_GRACE: TimeDelta = TimeDelta::minutes(5);
/// How long the log is still read once the agent exited.
const DRAIN: Duration = Duration::from_secs(2);

/// Sees runs through. Cheap to clone.
#[derive(Clone)]
pub struct Orchestrator {
    db: PgPool,
    events: EventBus,
    deps: RunsDeps,
    runtime: Arc<dyn SandboxRuntime>,
    api: url::Url,
}

impl std::fmt::Debug for Orchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orchestrator")
            .field("runtime", &self.runtime.name())
            .finish_non_exhaustive()
    }
}

/// How waiting for a run ended.
enum End {
    Exited(Option<i32>),
    Deadline,
    Cancelled,
    /// Its claim is gone: someone else ended it.
    Lost,
    Failed(String),
}

impl Orchestrator {
    /// An orchestrator over `deps`' runtime; none without one.
    pub fn new(db: PgPool, events: EventBus, deps: RunsDeps) -> Option<Self> {
        let runtime = deps.runtime.clone()?;
        let api = deps.api.clone()?;
        Some(Self {
            db,
            events,
            deps,
            runtime,
            api,
        })
    }

    /// Its runtime.
    pub fn runtime(&self) -> &Arc<dyn SandboxRuntime> {
        &self.runtime
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    /// End the runs whose server died, then remove every sandbox whose run
    /// is not going.
    pub async fn startup(&self) {
        self.reap().await;
        self.sweep().await;
    }

    /// End the runs whose claim lapsed. How many.
    pub async fn reap(&self) -> usize {
        match store::reap(&self.db, &self.events, self.now()).await {
            Ok(reaped) => reaped,
            Err(e) => {
                tracing::warn!(error = %e, "could not end the runs of a stopped server");
                0
            }
        }
    }

    /// Remove every Applicamp sandbox whose run is not going. How many.
    pub async fn sweep(&self) -> usize {
        let keep = match store::live(&self.db).await {
            Ok(keep) => keep,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the runs going; no sweep");
                return 0;
            }
        };
        match self.runtime.sweep(&keep).await {
            Ok(0) => 0,
            Ok(swept) => {
                tracing::info!(swept, "removed left-over sandboxes");
                swept
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not sweep left-over sandboxes");
                0
            }
        }
    }

    /// Claim one queued run that may start and see it through in the
    /// background. Its task, when there was one.
    pub async fn tick(&self) -> Result<Option<tokio::task::JoinHandle<()>>, sqlx::Error> {
        let now = self.now();
        let Some(run) = store::claim(
            &self.db,
            &self.deps.instance,
            self.deps.config.max_runs(),
            now,
            now + LEASE,
        )
        .await?
        else {
            return Ok(None);
        };
        tracing::info!(run = %run.id, kind = %run.kind, "run claimed");
        let this = self.clone();
        Ok(Some(tokio::spawn(async move { this.execute(run).await })))
    }

    /// Claim and run until `shutdown`.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut last_sweep = tokio::time::Instant::now();
        loop {
            if *shutdown.borrow() {
                return;
            }
            if self.reap().await > 0 {
                self.sweep().await;
            }
            if last_sweep.elapsed() >= SWEEP_EVERY {
                last_sweep = tokio::time::Instant::now();
                self.sweep().await;
            }
            let started = match self.tick().await {
                Ok(started) => started.is_some(),
                Err(e) => {
                    tracing::warn!(error = %e, "the orchestrator could not reach the database");
                    false
                }
            };
            if !started {
                tokio::select! {
                    () = tokio::time::sleep(POLL) => {}
                    _ = shutdown.changed() => {}
                }
            }
        }
    }

    async fn end(&self, run: Uuid, outcome: Outcome, error_kind: Option<&str>) {
        if let Err(e) =
            store::finish(&self.db, &self.events, run, outcome, error_kind, self.now()).await
        {
            // The claim lapses and the reaper ends it.
            tracing::error!(run = %run, error = %e, "could not record a run's end");
        }
    }

    /// See claimed run `run` through.
    pub async fn execute(&self, run: RunRow) {
        let Some(kind) = self.deps.kinds.get(&run.kind).cloned() else {
            tracing::warn!(run = %run.id, kind = %run.kind, "a run of a kind this server does not know");
            self.end(run.id, Outcome::Failed, Some("unknown_kind"))
                .await;
            return;
        };
        let now = self.now();
        let wall_clock = TimeDelta::from_std(kind.limits.wall_clock).unwrap_or(TimeDelta::days(1));
        let deadline = now + wall_clock;
        let until = tokio::time::Instant::now() + kind.limits.wall_clock;
        let token = crypto::random_token();
        let tools: Map<String, Value> = kind
            .tools
            .iter()
            .map(|tool| (tool.name.to_owned(), json!(tool.cap)))
            .collect();
        let image = self.deps.config.agent_image().to_owned();
        let policy = format!(
            "policies/{}.yaml",
            kind.name.split_once('.').map_or(kind.name, |(_, k)| k)
        );
        if let Err(e) = store::start(
            &self.db,
            run.id,
            deadline,
            &image,
            &policy,
            &crypto::hash(&token),
            &Value::Object(tools),
            deadline + TOKEN_GRACE,
        )
        .await
        {
            tracing::error!(run = %run.id, error = %e, "could not start a run");
            self.end(run.id, Outcome::Failed, Some("start_failed"))
                .await;
            return;
        }
        let spec = SandboxSpec {
            run: run.id,
            kind: kind.name,
            image,
            policy: kind.policy,
            api: self.api.clone(),
            token,
        };
        let handle = match self.runtime.create(spec).await {
            Ok(handle) => handle,
            Err(e) => {
                tracing::warn!(run = %run.id, kind = kind.name, error = %e, "could not create a sandbox");
                self.end(run.id, Outcome::Failed, Some("sandbox_create_failed"))
                    .await;
                return;
            }
        };
        let end = self.watch(&run, &handle, until).await;
        // Always, whatever happened.
        if let Err(e) = self.runtime.delete(&handle).await {
            tracing::error!(run = %run.id, error = %e, "could not delete a sandbox; the sweep will");
        }
        let (outcome, error_kind) = match end {
            End::Lost => return,
            End::Deadline => (Outcome::EndedTimeout, None),
            End::Cancelled => (Outcome::Cancelled, None),
            End::Failed(why) => {
                tracing::warn!(run = %run.id, error = %why, "lost track of a sandbox");
                (Outcome::Failed, Some("runtime_failed"))
            }
            End::Exited(code) => match store::fetch_any(&self.db, run.id).await {
                Ok(Some(row)) => match row.outcome() {
                    Some(outcome) => (outcome, None),
                    None => {
                        tracing::info!(run = %run.id, code, "the agent exited without finishing");
                        (Outcome::Failed, Some("agent_exited"))
                    }
                },
                _ => (Outcome::Failed, Some("agent_exited")),
            },
        };
        self.end(run.id, outcome, error_kind).await;
    }

    /// Follow the run while its sandbox exists: its log into its events,
    /// its claim renewed, until the agent exits, the deadline, or Stop.
    async fn watch(
        &self,
        run: &RunRow,
        handle: &SandboxHandle,
        until: tokio::time::Instant,
    ) -> End {
        if let Err(e) = store::running(&self.db, run.id, &handle.name).await {
            tracing::warn!(run = %run.id, error = %e, "could not mark a run running");
        }
        let follower = tokio::spawn(follow(self.db.clone(), run.id, self.runtime.logs(handle)));
        let stop = follower.abort_handle();
        let end = tokio::select! {
            exit = self.runtime.wait(handle, until) => match exit {
                Ok(Exit::Exited(code)) => End::Exited(code),
                Ok(Exit::Deadline) => End::Deadline,
                Err(e) => End::Failed(e.to_string()),
            },
            end = self.keep(run.id) => end,
        };
        if matches!(end, End::Exited(_)) {
            // What it wrote last.
            let _ = tokio::time::timeout(DRAIN, follower).await;
        }
        stop.abort();
        end
    }

    /// Renew run `run`'s claim every tick until someone asks it to stop or
    /// the claim is gone.
    async fn keep(&self, run: Uuid) -> End {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            match store::renew(&self.db, run, &self.deps.instance, self.now() + LEASE).await {
                Ok(Some(true)) => return End::Cancelled,
                Ok(Some(false)) => {}
                Ok(None) => return End::Lost,
                Err(e) => tracing::warn!(run = %run, error = %e, "could not renew a run's claim"),
            }
        }
    }
}

/// An event line as the runner writes it (on its output, and through the
/// agent API).
#[derive(Debug, Deserialize)]
pub(crate) struct AgentLine {
    pub seq: i64,
    pub level: Level,
    pub message: String,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

/// A sandbox log line as an event: the runner's JSON lines as agent
/// events, the proxy's lines as sandbox events (`deny` when it refused).
/// Anything else is not kept.
pub fn event(line: &LogLine) -> Option<NewEvent> {
    match line {
        LogLine::Agent(raw) => {
            let line: AgentLine = serde_json::from_str(raw).ok()?;
            (line.seq > 0).then(|| NewEvent {
                layer: Layer::Agent,
                level: line.level,
                agent_seq: Some(line.seq),
                operation: line.operation,
                tool_name: line.tool_name,
                message: line.message,
                data: line
                    .data
                    .filter(|data| data.to_string().len() <= crate::features::agent_api::MAX_DATA),
            })
        }
        LogLine::Sandbox(message) => {
            let denied = message.split_whitespace().any(|word| word == "DENIED");
            let operation = message
                .split_whitespace()
                .next()
                .filter(|word| word.contains(':'))
                .map(str::to_owned);
            Some(NewEvent {
                layer: Layer::Sandbox,
                level: if denied { Level::Deny } else { Level::Info },
                agent_seq: None,
                operation,
                tool_name: None,
                message: message.clone(),
                data: None,
            })
        }
    }
}

/// Record a sandbox's log as run `run`'s events until it ends.
async fn follow(
    db: PgPool,
    run: Uuid,
    mut lines: futures::stream::BoxStream<'static, crate::runtime::Result<LogLine>>,
) {
    while let Some(line) = lines.next().await {
        match line {
            Ok(line) => {
                if let Some(event) = event(&line)
                    && let Err(e) = store::insert_event(&db, run, Utc::now(), &event).await
                {
                    tracing::warn!(run = %run, error = %e, "could not record a run event");
                }
            }
            Err(e) => {
                tracing::warn!(run = %run, error = %e, "the sandbox log stopped");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_lines_become_agent_events() {
        let event = event(&LogLine::Agent(
            r#"{"seq":3,"level":"tool","message":"Called probe_echo","tool_name":"probe_echo"}"#
                .into(),
        ))
        .unwrap();
        assert_eq!(event.layer, Layer::Agent);
        assert_eq!(event.level, Level::Tool);
        assert_eq!(event.agent_seq, Some(3));
        assert_eq!(event.tool_name.as_deref(), Some("probe_echo"));
        assert!(super::event(&LogLine::Agent("plain text".into())).is_none());
        assert!(
            super::event(&LogLine::Agent(
                r#"{"seq":0,"level":"info","message":"x"}"#.into()
            ))
            .is_none()
        );
        assert!(
            super::event(&LogLine::Agent(
                r#"{"seq":1,"level":"shout","message":"x"}"#.into()
            ))
            .is_none()
        );
    }

    #[test]
    fn proxy_lines_become_sandbox_events() {
        let denied = event(&LogLine::Sandbox(
            "NET:OPEN [MED] DENIED applicamp-agent(7) -> example.com:443 [policy:- engine:opa]"
                .into(),
        ))
        .unwrap();
        assert_eq!((denied.layer, denied.level), (Layer::Sandbox, Level::Deny));
        assert_eq!(denied.operation.as_deref(), Some("NET:OPEN"));
        let allowed = event(&LogLine::Sandbox(
            "HTTP:POST [INFO] ALLOWED applicamp-agent(7) -> POST https://applicamp.example.com/api/agent/v1/events".into(),
        ))
        .unwrap();
        assert_eq!(allowed.level, Level::Info);
        assert_eq!(allowed.operation.as_deref(), Some("HTTP:POST"));
    }
}
