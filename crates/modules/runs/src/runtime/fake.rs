//! The fake runtime: the runner as a local child process, for tests and
//! local development. **Not a sandbox**: [`crate::config::RunsConfig::check`]
//! refuses it in production.
//!
//! The child starts with an empty environment (nothing of the server's
//! reaches it), its token on its standard input, and a local proxy as its
//! `HTTPS_PROXY`. The proxy applies the kind's policy hosts: a `CONNECT` to
//! one of them is tunnelled, anything else is refused with 403 and an
//! OpenShell-style `DENIED` line in the sandbox log. The agent API is
//! reached directly (`NO_PROXY`). Logs are the child's output lines and the
//! proxy's lines.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex, PoisonError},
};

use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{Notify, watch},
    task::JoinHandle,
    time::Instant,
};
use uuid::Uuid;

use super::{
    Exit, LOG_BUFFER, LogLine, LogSender, Result, RuntimeError, SandboxHandle, SandboxRuntime,
    SandboxSpec, TokenArg, agent_args, policy,
};
use crate::kinds::Kinds;

/// Longest request head the proxy reads.
const MAX_HEAD: usize = 8 * 1024;

/// One running child.
struct Sandbox {
    run: Uuid,
    exit: watch::Receiver<Option<Option<i32>>>,
    kill: Arc<Notify>,
    lines: Option<BoxStream<'static, Result<LogLine>>>,
    proxy: JoinHandle<()>,
}

/// The fake runtime.
pub struct FakeRuntime {
    program: PathBuf,
    prefix: Vec<String>,
    sandboxes: Mutex<HashMap<String, Sandbox>>,
}

impl std::fmt::Debug for FakeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeRuntime")
            .field("program", &self.program)
            .finish_non_exhaustive()
    }
}

impl FakeRuntime {
    /// Children run `agent` (the `applicamp-agent` binary).
    pub fn new(agent: PathBuf) -> Self {
        Self::with_command(agent, Vec::new())
    }

    /// Children run `program` with `prefix` before the runner's arguments
    /// (tests: `sh -c '<script>' sh`).
    pub fn with_command(program: PathBuf, prefix: Vec<String>) -> Self {
        Self {
            program,
            prefix,
            sandboxes: Mutex::new(HashMap::new()),
        }
    }

    /// How many sandboxes exist.
    pub fn len(&self) -> usize {
        self.sandboxes().len()
    }

    /// Whether none exists.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn sandboxes(&self) -> std::sync::MutexGuard<'_, HashMap<String, Sandbox>> {
        self.sandboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl SandboxRuntime for FakeRuntime {
    fn name(&self) -> &'static str {
        "fake"
    }

    async fn prepare(&self, _kinds: &Kinds) -> Result<()> {
        Ok(())
    }

    async fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle> {
        let policy = policy::for_run(spec.policy, &spec.api).map_err(RuntimeError)?;
        let allowed: HashSet<(String, u16)> = policy::hosts(&policy).into_iter().collect();
        let (lines, rx) = LogSender::new(LOG_BUFFER);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| RuntimeError(format!("the fake proxy could not listen: {e}")))?;
        let proxy_addr = listener
            .local_addr()
            .map_err(|e| RuntimeError(format!("the fake proxy has no address: {e}")))?;
        let proxy = tokio::spawn(serve_proxy(
            listener,
            Arc::new(allowed),
            spec.kind,
            lines.clone(),
        ));
        let proxy_url = format!("http://{proxy_addr}");
        let api_host = spec.api.host_str().unwrap_or("localhost").to_owned();

        let mut child = Command::new(&self.program)
            .args(&self.prefix)
            .args(agent_args(&spec, TokenArg::Stdin))
            .env_clear()
            .env("HTTPS_PROXY", &proxy_url)
            .env("HTTP_PROXY", &proxy_url)
            .env("NO_PROXY", &api_host)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                proxy.abort();
                RuntimeError(format!(
                    "the fake runtime could not start {}: {e}",
                    self.program.display()
                ))
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            // A child that does not read its token simply never gets it.
            let _ = stdin.write_all(spec.token.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        if let Some(stdout) = child.stdout.take() {
            let lines = lines.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !lines.send(Ok(LogLine::Agent(line))) {
                        break;
                    }
                }
            });
        }
        let (exit_tx, exit) = watch::channel(None);
        let kill = Arc::new(Notify::new());
        let killed = Arc::clone(&kill);
        tokio::spawn(async move {
            let code = tokio::select! {
                status = child.wait() => status.ok().and_then(|status| status.code()),
                () = killed.notified() => {
                    let _ = child.kill().await;
                    None
                }
            };
            let _ = exit_tx.send(Some(code));
        });
        let name = format!("fake-{}", spec.run.simple());
        self.sandboxes().insert(
            name.clone(),
            Sandbox {
                run: spec.run,
                exit,
                kill,
                lines: Some(rx),
                proxy,
            },
        );
        Ok(SandboxHandle { name })
    }

    fn logs(&self, handle: &SandboxHandle) -> BoxStream<'static, Result<LogLine>> {
        let rx = self
            .sandboxes()
            .get_mut(&handle.name)
            .and_then(|sandbox| sandbox.lines.take());
        rx.unwrap_or_else(|| futures::stream::empty().boxed())
    }

    async fn wait(&self, handle: &SandboxHandle, until: Instant) -> Result<Exit> {
        let Some(mut exit) = self
            .sandboxes()
            .get(&handle.name)
            .map(|sandbox| sandbox.exit.clone())
        else {
            return Ok(Exit::Exited(None));
        };
        match tokio::time::timeout_at(until, exit.wait_for(Option::is_some)).await {
            Ok(Ok(code)) => Ok(Exit::Exited((*code).flatten())),
            Ok(Err(_)) => Ok(Exit::Exited(None)),
            Err(_) => Ok(Exit::Deadline),
        }
    }

    async fn delete(&self, handle: &SandboxHandle) -> Result<()> {
        let removed = self.sandboxes().remove(&handle.name);
        if let Some(sandbox) = removed {
            sandbox.kill.notify_one();
            sandbox.proxy.abort();
        }
        Ok(())
    }

    async fn sweep(&self, keep: &HashSet<Uuid>) -> Result<usize> {
        let gone: Vec<String> = self
            .sandboxes()
            .iter()
            .filter(|(_, sandbox)| !keep.contains(&sandbox.run))
            .map(|(name, _)| name.clone())
            .collect();
        for name in &gone {
            self.delete(&SandboxHandle { name: name.clone() }).await?;
        }
        Ok(gone.len())
    }
}

/// Accept connections until aborted.
async fn serve_proxy(
    listener: TcpListener,
    allowed: Arc<HashSet<(String, u16)>>,
    kind: &'static str,
    lines: LogSender,
) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(proxy_one(stream, Arc::clone(&allowed), kind, lines.clone()));
    }
}

/// What a request head asks the proxy for.
#[derive(Debug, PartialEq, Eq)]
struct Asked {
    method: String,
    host: String,
    port: u16,
}

/// The method and destination of a proxy request head (`CONNECT host:port`
/// or `GET http://host/…`).
fn parse_head(head: &str) -> Option<Asked> {
    let line = head.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_ascii_uppercase();
    let target = parts.next()?;
    let (host, port) = if method == "CONNECT" {
        let (host, port) = target.rsplit_once(':')?;
        (host.to_owned(), port.parse().ok()?)
    } else {
        let url = url::Url::parse(target).ok()?;
        (url.host_str()?.to_owned(), url.port_or_known_default()?)
    };
    Some(Asked {
        method,
        host: host.trim_matches(['[', ']']).to_ascii_lowercase(),
        port,
    })
}

/// The OpenShell-style shorthand line for a decision.
fn line(asked: &Asked, allowed: bool, kind: &str) -> String {
    let (severity, action) = if allowed {
        ("[INFO]", "ALLOWED")
    } else {
        ("[MED]", "DENIED")
    };
    let class = if asked.method == "CONNECT" {
        "NET:OPEN".to_owned()
    } else {
        format!("HTTP:{}", asked.method)
    };
    format!(
        "{class} {severity} {action} applicamp-agent -> {}:{} [policy:{kind} engine:fake]",
        asked.host, asked.port
    )
}

async fn proxy_one(
    mut stream: TcpStream,
    allowed: Arc<HashSet<(String, u16)>>,
    kind: &'static str,
    lines: LogSender,
) {
    let mut head = Vec::new();
    let mut buf = [0_u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
        if head.len() > MAX_HEAD {
            return;
        }
    }
    let Some(asked) = parse_head(&String::from_utf8_lossy(&head)) else {
        let _ = stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    };
    let ok = allowed.contains(&(asked.host.clone(), asked.port));
    lines.send(Ok(LogLine::Sandbox(line(&asked, ok, kind))));
    if !ok {
        let _ = stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        return;
    }
    if asked.method != "CONNECT" {
        // The agent API is reached directly; nothing else is plain http.
        let _ = stream
            .write_all(b"HTTP/1.1 501 Not Implemented\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    }
    let Ok(mut upstream) = TcpStream::connect((asked.host.as_str(), asked.port)).await else {
        let _ = stream
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    };
    if stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .is_ok()
    {
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt as _;

    use super::*;

    #[test]
    fn proxy_heads_parse() {
        assert_eq!(
            parse_head("CONNECT Example.com:443 HTTP/1.1\r\nHost: example.com\r\n\r\n"),
            Some(Asked {
                method: "CONNECT".into(),
                host: "example.com".into(),
                port: 443
            })
        );
        assert_eq!(
            parse_head("GET http://example.com/x HTTP/1.1\r\n\r\n"),
            Some(Asked {
                method: "GET".into(),
                host: "example.com".into(),
                port: 80
            })
        );
        assert_eq!(parse_head("CONNECT nonsense HTTP/1.1\r\n\r\n"), None);
        assert_eq!(parse_head(""), None);
    }

    #[test]
    fn decisions_read_like_openshell() {
        let asked = Asked {
            method: "CONNECT".into(),
            host: "example.com".into(),
            port: 443,
        };
        let denied = line(&asked, false, "runs.probe");
        assert!(denied.starts_with("NET:OPEN [MED] DENIED"), "{denied}");
        assert!(denied.contains("-> example.com:443"));
        assert!(line(&asked, true, "runs.probe").contains(" ALLOWED "));
    }

    #[tokio::test]
    async fn the_proxy_refuses_hosts_outside_the_policy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (lines, mut rx) = LogSender::new(8);
        let proxy = tokio::spawn(serve_proxy(
            listener,
            Arc::new(HashSet::new()),
            "runs.probe",
            lines,
        ));
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();
        let mut answer = String::new();
        client.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
        let Some(Ok(LogLine::Sandbox(line))) = rx.next().await else {
            panic!("no sandbox line");
        };
        assert!(line.contains("DENIED"), "{line}");
        proxy.abort();
    }
}
