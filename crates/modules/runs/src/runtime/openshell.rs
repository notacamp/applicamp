//! NVIDIA OpenShell through its gateway (`openshell-sdk`, tag v0.1.6).
//!
//! - **Templates**: one per kind, `applicamp-<module>-<kind>`, carrying the
//!   image and the resources (1 CPU, 1 GiB). Made at startup; the gateway
//!   has no update call, so a template that differs is deleted and made
//!   again. A template carries no per-run environment, so the run id, its
//!   token and the agent API reach the runner as its command's arguments.
//! - **Policy**: the kind's file, parsed with OpenShell's parser, set on
//!   each sandbox when it is created ([`super::policy::for_run`]).
//! - **Logs**: `watch_logs` (it resumes after a dropped stream by itself);
//!   the network proxy's lines arrive with level `OCSF`.
//! - **Auth**: a bearer this server mints for itself ([`crate::issuer`]),
//!   refreshed before it expires.
//!
//! Sandboxes are labelled `applicamp=run` and `applicamp-run=<id>`, so the
//! sweep finds the ones whose run is over.

use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Duration};

use applicamp_kernel::agent::RunKind;
use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use openshell_sdk::{
    AuthConfig, ClientConfig, DeleteOptions, ListOptions, OpenShellClient, Refresh, RefreshError,
    RefreshedToken, SandboxPhase, SandboxResources, SandboxTemplateCreateSpec,
    SandboxWorkloadConfig, SandboxWorkloadTemplate, SandboxWorkloadTemplateSpec, SdkError,
    WatchEvent, WatchOptions, raw::proto::datamodel::v1::ObjectMeta,
};
use tokio::{sync::OnceCell, time::Instant};
use uuid::Uuid;

use super::{
    AGENT_BINARY, Exit, LABEL, LABEL_VALUE, LogLine, RUN_LABEL, Result, RuntimeError,
    SandboxHandle, SandboxRuntime, SandboxSpec, TokenArg, agent_args, policy,
};
use crate::{issuer::Issuer, kinds::Kinds};

/// A sandbox's CPU.
pub const CPU: &str = "1";
/// A sandbox's memory.
pub const MEMORY: &str = "1Gi";
/// How often a run's sandbox is looked at while it goes.
const POLL: Duration = Duration::from_secs(1);
/// Failed looks in a row before waiting gives up.
const MAX_FAILED_POLLS: u32 = 10;

/// Mints the bearer of every gateway call.
struct GatewayToken(Arc<Issuer>);

impl GatewayToken {
    fn mint(&self) -> std::result::Result<(String, u64), String> {
        let now = u64::try_from(chrono::Utc::now().timestamp()).unwrap_or_default();
        self.0.mint(now)
    }
}

#[async_trait]
impl Refresh for GatewayToken {
    async fn refresh(&self) -> std::result::Result<RefreshedToken, RefreshError> {
        let (token, exp) = self.mint().map_err(RefreshError::Terminal)?;
        Ok(RefreshedToken::new(token).with_expires_at(exp))
    }
}

/// The OpenShell runtime.
pub struct OpenShellRuntime {
    gateway: String,
    ca: Option<Vec<u8>>,
    token: Arc<GatewayToken>,
    image: String,
    client: OnceCell<OpenShellClient>,
}

impl std::fmt::Debug for OpenShellRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenShellRuntime")
            .field("gateway", &self.gateway)
            .field("image", &self.image)
            .finish_non_exhaustive()
    }
}

impl OpenShellRuntime {
    /// The gateway at `gateway` (with its private CA, when pinned),
    /// authenticated by `issuer`'s tokens, running the runner's `image`.
    /// Connects on first use.
    pub fn new(gateway: String, ca: Option<Vec<u8>>, issuer: Arc<Issuer>, image: String) -> Self {
        Self {
            gateway,
            ca,
            token: Arc::new(GatewayToken(issuer)),
            image,
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> Result<&OpenShellClient> {
        self.client
            .get_or_try_init(|| async {
                let (token, exp) = self.token.mint().map_err(RuntimeError)?;
                let mut config = ClientConfig::new(self.gateway.clone());
                config.ca_cert.clone_from(&self.ca);
                let refresh: Arc<dyn Refresh> = self.token.clone();
                config.auth = Some(AuthConfig::Oidc {
                    token,
                    expires_at: Some(exp),
                    refresh: Some(refresh),
                });
                OpenShellClient::connect(config)
                    .await
                    .map_err(|e| failed("connect to the gateway", &e))
            })
            .await
    }
}

/// A gateway error, without anything it echoes of the request.
fn failed(what: &str, err: &SdkError) -> RuntimeError {
    let kind = match err {
        SdkError::InvalidConfig { .. } => "invalid configuration",
        SdkError::Tls { .. } => "TLS",
        SdkError::Connect { .. } => "unreachable",
        SdkError::Auth { .. } => "not authenticated",
        SdkError::Io { .. } => "I/O",
        SdkError::NotFound { .. } => "not found",
        SdkError::AlreadyExists { .. } => "already exists",
        SdkError::OutOfRange { .. } => "log out of range",
        SdkError::Rpc { .. } => "gateway error",
    };
    RuntimeError(format!("could not {what}: {kind} ({err})"))
}

/// A kind's template name: `runs.probe` is `applicamp-runs-probe`.
pub fn template_name(kind: &str) -> String {
    let slug: String = kind
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("applicamp-{slug}")
}

/// The template a kind's sandboxes are made from.
pub fn template(kind: &RunKind, image: &str) -> SandboxWorkloadTemplate {
    SandboxWorkloadTemplate {
        metadata: Some(ObjectMeta {
            name: template_name(kind.name),
            ..ObjectMeta::default()
        }),
        spec: Some(SandboxWorkloadTemplateSpec {
            workload: Some(SandboxWorkloadConfig {
                image: image.to_owned(),
                environment: HashMap::new(),
                resources: Some(SandboxResources {
                    cpu: CPU.to_owned(),
                    memory: MEMORY.to_owned(),
                    gpu: None,
                }),
            }),
            ..SandboxWorkloadTemplateSpec::default()
        }),
    }
}

/// Whether the template there makes the same sandboxes as `want`: its image,
/// environment and resources.
pub fn same_template(there: &SandboxWorkloadTemplate, want: &SandboxWorkloadTemplate) -> bool {
    let workload = |t: &SandboxWorkloadTemplate| t.spec.as_ref().and_then(|s| s.workload.clone());
    workload(there) == workload(want)
}

/// A gateway log line as a sandbox log line: the proxy's OCSF lines, and
/// the runner's own JSON lines; nothing else (the supervisor's own lines).
pub fn log_line(level: &str, message: &str) -> Option<LogLine> {
    if level.eq_ignore_ascii_case("OCSF") {
        return Some(LogLine::Sandbox(message.to_owned()));
    }
    let trimmed = message.trim();
    let json = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    json.get("seq")?.as_u64()?;
    Some(LogLine::Agent(trimmed.to_owned()))
}

#[async_trait]
impl SandboxRuntime for OpenShellRuntime {
    fn name(&self) -> &'static str {
        "openshell"
    }

    async fn prepare(&self, kinds: &Kinds) -> Result<()> {
        let client = self.client().await?;
        for kind in kinds.iter() {
            let want = template(kind, &self.image);
            let name = template_name(kind.name);
            match client.get_sandbox_template(&name).await {
                Ok(there) if same_template(&there, &want) => continue,
                Ok(_) => {
                    client
                        .delete_sandbox_template(
                            &name,
                            DeleteOptions {
                                allow_missing: true,
                            },
                        )
                        .await
                        .map_err(|e| failed("replace a template", &e))?;
                }
                Err(SdkError::NotFound { .. }) => {}
                Err(e) => return Err(failed("read a template", &e)),
            }
            client
                .create_sandbox_template(want)
                .await
                .map_err(|e| failed("create a template", &e))?;
            tracing::info!(kind = kind.name, template = %name, "sandbox template made");
        }
        Ok(())
    }

    async fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle> {
        let client = self.client().await?;
        let policy = policy::for_run(spec.policy, &spec.api).map_err(RuntimeError)?;
        let mut command = vec![AGENT_BINARY.to_owned()];
        command.extend(agent_args(&spec, TokenArg::Inline(&spec.token)));
        let labels = HashMap::from([
            (LABEL.to_owned(), LABEL_VALUE.to_owned()),
            (RUN_LABEL.to_owned(), spec.run.to_string()),
        ]);
        let sandbox = client
            .create_sandbox_from_template(SandboxTemplateCreateSpec {
                template_name: template_name(spec.kind),
                labels,
                command,
                policy: Some(policy),
                ..SandboxTemplateCreateSpec::default()
            })
            .await
            .map_err(|e| failed("create a sandbox", &e))?;
        Ok(SandboxHandle { name: sandbox.name })
    }

    fn logs(&self, handle: &SandboxHandle) -> BoxStream<'static, Result<LogLine>> {
        let Some(client) = self.client.get().cloned() else {
            return futures::stream::once(async {
                Err(RuntimeError("the gateway is not connected".to_owned()))
            })
            .boxed();
        };
        let name = handle.name.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let options = WatchOptions {
                follow_logs: true,
                ..WatchOptions::default()
            };
            let stream = client.watch_logs(&name, options);
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let line = match item {
                    Ok(WatchEvent::Log { line, .. }) => {
                        match log_line(&line.level, &line.message) {
                            Some(line) => Ok(line),
                            None => continue,
                        }
                    }
                    Ok(_) => continue,
                    Err(e) => Err(failed("follow the sandbox log", &e)),
                };
                let end = line.is_err();
                if tx.send(line).is_err() || end {
                    break;
                }
            }
        });
        futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|line| (line, rx))
        })
        .boxed()
    }

    async fn wait(&self, handle: &SandboxHandle, until: Instant) -> Result<Exit> {
        let client = self.client().await?;
        let mut failures = 0;
        loop {
            match client.get_sandbox(&handle.name).await {
                Ok(sandbox) => {
                    failures = 0;
                    if matches!(
                        sandbox.phase,
                        SandboxPhase::Completed | SandboxPhase::Stopped | SandboxPhase::Error
                    ) {
                        return Ok(Exit::Exited(sandbox.exit_code));
                    }
                }
                Err(SdkError::NotFound { .. }) => return Ok(Exit::Exited(None)),
                Err(e) => {
                    failures += 1;
                    tracing::warn!(error = %failed("read a sandbox", &e), "the gateway did not answer");
                    if failures >= MAX_FAILED_POLLS {
                        return Err(failed("read a sandbox", &e));
                    }
                }
            }
            let now = Instant::now();
            if now >= until {
                return Ok(Exit::Deadline);
            }
            tokio::time::sleep_until((now + POLL).min(until)).await;
        }
    }

    async fn delete(&self, handle: &SandboxHandle) -> Result<()> {
        let client = self.client().await?;
        client
            .delete_sandbox(
                &handle.name,
                DeleteOptions {
                    allow_missing: true,
                },
            )
            .await
            .map_err(|e| failed("delete a sandbox", &e))?;
        Ok(())
    }

    async fn sweep(&self, keep: &HashSet<Uuid>) -> Result<usize> {
        let client = self.client().await?;
        let sandboxes = client
            .list_all_sandboxes(ListOptions {
                label_selector: Some(format!("{LABEL}={LABEL_VALUE}")),
                ..ListOptions::default()
            })
            .await
            .map_err(|e| failed("list sandboxes", &e))?;
        let mut deleted = 0;
        for sandbox in sandboxes {
            let run = sandbox
                .labels
                .get(RUN_LABEL)
                .and_then(|id| Uuid::parse_str(id).ok());
            if run.is_some_and(|run| keep.contains(&run)) {
                continue;
            }
            self.delete(&SandboxHandle { name: sandbox.name }).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_named_after_their_kind() {
        assert_eq!(template_name("runs.probe"), "applicamp-runs-probe");
        assert_eq!(
            template_name("pipeline.company_research"),
            "applicamp-pipeline-company-research"
        );
    }

    #[test]
    fn a_template_is_replaced_only_when_it_differs() {
        let kind = crate::probe::kind();
        let want = template(&kind, "ghcr.io/notacamp/applicamp-agent:main");
        let mut there = want.clone();
        // The gateway's own metadata (ids, versions) does not matter.
        if let Some(meta) = there.metadata.as_mut() {
            meta.id = "tpl-1".to_owned();
            meta.resource_version = 7;
        }
        assert!(same_template(&there, &want));
        let newer = template(&kind, "ghcr.io/notacamp/applicamp-agent:main-abc");
        assert!(!same_template(&newer, &want));
        let workload = want
            .spec
            .as_ref()
            .and_then(|spec| spec.workload.as_ref())
            .unwrap();
        let resources = workload.resources.as_ref().unwrap();
        assert_eq!(
            (resources.cpu.as_str(), resources.memory.as_str()),
            ("1", "1Gi")
        );
        assert!(workload.environment.is_empty(), "no per-run environment");
    }

    #[test]
    fn the_proxy_lines_and_the_runner_lines_are_kept() {
        assert_eq!(
            log_line(
                "OCSF",
                "NET:OPEN [MED] DENIED applicamp-agent(7) -> example.com:443"
            ),
            Some(LogLine::Sandbox(
                "NET:OPEN [MED] DENIED applicamp-agent(7) -> example.com:443".to_owned()
            ))
        );
        assert_eq!(
            log_line("INFO", r#" {"seq":1,"level":"info","message":"hi"} "#),
            Some(LogLine::Agent(
                r#"{"seq":1,"level":"info","message":"hi"}"#.to_owned()
            ))
        );
        assert_eq!(log_line("INFO", "Landlock ruleset built"), None);
        assert_eq!(log_line("INFO", r#"{"no":"seq"}"#), None);
    }
}
