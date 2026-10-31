//! Where sandboxes come from: the [`SandboxRuntime`] trait, so OpenShell
//! stays behind it (ADR 0002: a gVisor fallback would be a second
//! implementation, not a rewrite).
//!
//! - [`openshell`]: NVIDIA OpenShell through its gateway's gRPC API.
//! - [`fake`]: the runner as a local child process behind a proxy that
//!   applies the kind's policy hosts. Tests and local development only.
//!
//! A sandbox receives only its run id, its single-run token and the agent
//! API's address (ADR 0003); never the server's environment.

pub mod fake;
pub mod openshell;
pub mod policy;

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use tokio::sync::mpsc;
use tokio::time::Instant;
use url::Url;
use uuid::Uuid;

use crate::kinds::Kinds;

/// The label every Applicamp sandbox carries (`applicamp=run`), so a sweep
/// finds them all.
pub const LABEL: &str = "applicamp";
/// Its value.
pub const LABEL_VALUE: &str = "run";
/// The label naming a sandbox's run (`applicamp-run=<id>`).
pub const RUN_LABEL: &str = "applicamp-run";
/// Where the runner lives in its image.
pub const AGENT_BINARY: &str = "/app/applicamp-agent";

/// A runtime failed. The message names what, never a token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RuntimeError(pub String);

/// Runtime result alias.
pub type Result<T, E = RuntimeError> = std::result::Result<T, E>;

/// A sandbox to create for one run.
#[derive(Clone)]
pub struct SandboxSpec {
    /// The run.
    pub run: Uuid,
    /// Its kind (`<module>.<kind>`).
    pub kind: &'static str,
    /// The image the runner runs in.
    pub image: String,
    /// The kind's policy (YAML, with the agent API placeholder).
    pub policy: &'static str,
    /// Where the agent reaches the agent API.
    pub api: Url,
    /// The run's token, sent once into the sandbox.
    pub token: String,
}

impl std::fmt::Debug for SandboxSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxSpec")
            .field("run", &self.run)
            .field("kind", &self.kind)
            .field("image", &self.image)
            .field("api", &self.api.as_str())
            .finish_non_exhaustive()
    }
}

/// A created sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxHandle {
    /// Its name at the runtime.
    pub name: String,
}

/// One line of a sandbox's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    /// A line the runner wrote to its output (a JSON event, or anything).
    Agent(String),
    /// An allow or deny line of the sandbox's network proxy (OpenShell's
    /// OCSF shorthand: `NET:OPEN [MED] DENIED … -> example.com:443`).
    Sandbox(String),
    /// This many lines came faster than they could be recorded and were
    /// dropped.
    Dropped(u64),
}

/// Lines a sandbox's log holds before it drops them.
pub const LOG_BUFFER: usize = 1024;

/// The sending side of a sandbox's log: bounded, so a runner writing faster
/// than lines are recorded loses lines (counted, and said so with a
/// [`LogLine::Dropped`]) instead of growing the server's memory. Cheap to
/// clone; clones share the count.
#[derive(Debug, Clone)]
pub struct LogSender {
    tx: mpsc::Sender<Result<LogLine>>,
    dropped: Arc<AtomicU64>,
}

impl LogSender {
    /// A log of at most `capacity` waiting lines, and its stream.
    pub fn new(capacity: usize) -> (Self, BoxStream<'static, Result<LogLine>>) {
        let (tx, rx) = mpsc::channel(capacity.max(2));
        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|line| (line, rx))
        })
        .boxed();
        (
            Self {
                tx,
                dropped: Arc::new(AtomicU64::new(0)),
            },
            stream,
        )
    }

    /// Send `line` unless the log is full (then it is counted as dropped).
    /// Whether anyone still reads the log.
    pub fn send(&self, line: Result<LogLine>) -> bool {
        match self.tx.try_send(line) {
            Ok(()) => {
                let dropped = self.dropped.swap(0, Ordering::SeqCst);
                if dropped > 0 && self.tx.try_send(Ok(LogLine::Dropped(dropped))).is_err() {
                    self.dropped.fetch_add(dropped, Ordering::SeqCst);
                }
                true
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::SeqCst);
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// How waiting for a sandbox ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Its runner exited, with its code when known.
    Exited(Option<i32>),
    /// The deadline came first.
    Deadline,
}

/// A source of sandboxes.
#[async_trait]
pub trait SandboxRuntime: Send + Sync + 'static {
    /// `openshell` or `fake`.
    fn name(&self) -> &'static str;

    /// At startup: make what every kind needs exist (OpenShell's templates).
    async fn prepare(&self, kinds: &Kinds) -> Result<()>;

    /// Create the sandbox of `spec` and start its runner.
    async fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle>;

    /// The sandbox's log from its start, until it is gone.
    fn logs(&self, handle: &SandboxHandle) -> BoxStream<'static, Result<LogLine>>;

    /// Wait for the runner to exit, at most until `until`.
    async fn wait(&self, handle: &SandboxHandle, until: Instant) -> Result<Exit>;

    /// Remove the sandbox (one already gone is fine).
    async fn delete(&self, handle: &SandboxHandle) -> Result<()>;

    /// Remove every Applicamp sandbox whose run is not in `keep`. How many.
    async fn sweep(&self, keep: &HashSet<Uuid>) -> Result<usize>;
}

/// The runner's arguments, after its binary: the run, the agent API and
/// where its token comes from.
pub fn agent_args(spec: &SandboxSpec, token: TokenArg<'_>) -> Vec<String> {
    let mut args = vec![
        "--run".to_owned(),
        spec.run.to_string(),
        "--api".to_owned(),
        spec.api.to_string(),
    ];
    match token {
        TokenArg::Inline(token) => args.extend(["--token".to_owned(), token.to_owned()]),
        TokenArg::Stdin => args.extend(["--token-file".to_owned(), "-".to_owned()]),
    }
    args
}

/// How the runner receives its token.
#[derive(Debug, Clone, Copy)]
pub enum TokenArg<'a> {
    /// As an argument (OpenShell: a template carries no per-run
    /// environment and nothing else reaches the sandbox).
    Inline(&'a str),
    /// On its standard input (the fake runtime).
    Stdin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_full_log_drops_lines_and_says_how_many() {
        let (log, mut lines) = LogSender::new(2);
        for n in 0..5 {
            assert!(log.send(Ok(LogLine::Agent(n.to_string()))));
        }
        assert_eq!(lines.next().await, Some(Ok(LogLine::Agent("0".into()))));
        assert_eq!(lines.next().await, Some(Ok(LogLine::Agent("1".into()))));
        // Room again: the next line goes, then how many were lost.
        assert!(log.send(Ok(LogLine::Agent("5".into()))));
        assert_eq!(lines.next().await, Some(Ok(LogLine::Agent("5".into()))));
        assert_eq!(lines.next().await, Some(Ok(LogLine::Dropped(3))));
        drop(lines);
        assert!(!log.send(Ok(LogLine::Agent("6".into()))), "nobody reads it");
    }

    #[test]
    fn the_runner_gets_its_run_api_and_token_and_nothing_prints_the_token() {
        let spec = SandboxSpec {
            run: Uuid::nil(),
            kind: "runs.probe",
            image: "agent".to_owned(),
            policy: "",
            api: Url::parse("https://applicamp.example.com").unwrap(),
            token: "s3cret".to_owned(),
        };
        assert!(!format!("{spec:?}").contains("s3cret"));
        assert_eq!(
            agent_args(&spec, TokenArg::Stdin),
            [
                "--run",
                "00000000-0000-0000-0000-000000000000",
                "--api",
                "https://applicamp.example.com/",
                "--token-file",
                "-"
            ]
        );
        assert_eq!(agent_args(&spec, TokenArg::Inline("s3cret"))[5], "s3cret");
    }
}
