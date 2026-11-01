//! NVIDIA OpenShell through its gateway (`openshell-sdk`, tag v0.1.6).
//!
//! - **Templates**: one per kind, named after it ([`template_name`]),
//!   carrying the image and the resources (1 CPU, 1 GiB). Made at startup
//!   (and again until the gateway takes them, [`crate::start`]); the gateway
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
    AGENT_BINARY, Exit, LABEL, LABEL_VALUE, LOG_BUFFER, LogLine, LogSender, RUN_LABEL, Result,
    RuntimeError, SandboxHandle, SandboxRuntime, SandboxSpec, TokenArg, agent_args, policy,
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
    /// The bearer of every call; none only for the contract test's
    /// unauthenticated gateway.
    token: Option<Arc<GatewayToken>>,
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
            token: Some(Arc::new(GatewayToken(issuer))),
            image,
            client: OnceCell::new(),
        }
    }

    /// The gateway at `gateway` with no authentication: the contract test's
    /// plaintext gateway that lets unauthenticated users in. Never in a build.
    #[cfg(test)]
    pub(crate) fn anonymous(gateway: String, image: String) -> Self {
        Self {
            gateway,
            ca: None,
            token: None,
            image,
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> Result<&OpenShellClient> {
        self.client
            .get_or_try_init(|| async {
                let mut config = ClientConfig::new(self.gateway.clone());
                config.ca_cert.clone_from(&self.ca);
                if let Some(minter) = &self.token {
                    let (token, exp) = minter.mint().map_err(RuntimeError)?;
                    let refresh: Arc<dyn Refresh> = minter.clone();
                    config.auth = Some(AuthConfig::Oidc {
                        token,
                        expires_at: Some(exp),
                        refresh: Some(refresh),
                    });
                }
                OpenShellClient::connect(config)
                    .await
                    .map_err(|e| failed("connect to the gateway", &e))
            })
            .await
    }
}

/// A gateway error as its kind (and gRPC code) only: never its message,
/// which may echo the request, and a sandbox's request carries its run's
/// token in its command.
fn failed(what: &str, err: &SdkError) -> RuntimeError {
    let kind = match err {
        SdkError::InvalidConfig { .. } => "invalid configuration".to_owned(),
        SdkError::Tls { .. } => "TLS".to_owned(),
        SdkError::Connect { .. } => "unreachable".to_owned(),
        SdkError::Auth { .. } => "not authenticated".to_owned(),
        SdkError::Io { .. } => "I/O".to_owned(),
        SdkError::NotFound { .. } => "not found".to_owned(),
        SdkError::AlreadyExists { .. } => "already exists".to_owned(),
        SdkError::OutOfRange { .. } => "log out of range".to_owned(),
        SdkError::Rpc { code, .. } => format!("gateway error, gRPC code {code}"),
    };
    RuntimeError(format!("could not {what}: {kind}"))
}

/// The longest name the gateway takes for a template (v0.1.6 refuses a
/// longer one with "name exceeds maximum length").
pub const MAX_TEMPLATE_NAME: usize = 19;

/// A kind's template name, a DNS label the gateway takes: `runs.probe` is
/// `runs-probe`. A kind too long for [`MAX_TEMPLATE_NAME`] keeps its first
/// twelve characters and a hash of the whole kind, so two long kinds never
/// share a template: `pipeline.company_research` is `pipeline-com-` and six
/// hex digits.
pub fn template_name(kind: &str) -> String {
    let mut slug = String::with_capacity(kind.len());
    for c in kind.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.len() <= MAX_TEMPLATE_NAME {
        return slug;
    }
    let mut head: String = slug.chars().take(12).collect();
    while head.ends_with('-') {
        head.pop();
    }
    // FNV-1a: stable across builds and platforms, which `DefaultHasher` is not.
    let hash = kind.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    format!("{head}-{:06x}", hash & 0x00ff_ffff)
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
        // The token goes in the command: a template-made sandbox takes no
        // per-run environment (the gateway refuses `spec.environment` with a
        // template), OpenShell v0.1.6 has no way to put a file into it at
        // creation, and the canonical command gets no standard input. The
        // gateway keeps the command with the sandbox, which is deleted when
        // the run ends; the token dies with the run (deadline + 5 min at
        // most), and errors are logged by kind only ([`failed`]).
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
        let (tx, lines) = LogSender::new(LOG_BUFFER);
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
                if !tx.send(line) || end {
                    break;
                }
            }
        });
        lines
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
    fn gateway_errors_never_carry_their_message() {
        for err in [
            SdkError::invalid_config("spec.command: --token s3cret is too long"),
            SdkError::Connect {
                message: "reset while sending --token s3cret".to_owned(),
            },
        ] {
            let said = failed("create a sandbox", &err).to_string();
            assert!(said.starts_with("could not create a sandbox: "), "{said}");
            assert!(!said.contains("s3cret"), "{said}");
        }
    }

    #[test]
    fn templates_are_named_after_their_kind() {
        assert_eq!(template_name("runs.probe"), "runs-probe");
        assert_eq!(template_name("pipeline.prepare"), "pipeline-prepare");
        assert_eq!(template_name("Runs..Probe_"), "runs-probe");
    }

    #[test]
    fn every_template_name_is_one_the_gateway_takes() {
        let long = template_name("pipeline.company_research");
        assert!(long.starts_with("pipeline-com-"), "{long}");
        assert_eq!(long, template_name("pipeline.company_research"));
        assert_ne!(long, template_name("pipeline.company_reviews"));
        for kind in [
            "runs.probe",
            "pipeline.company_research",
            "pipeline.company_reviews",
            "sources.board_search",
            "a.b",
            "x_______________________y",
            "inbox.recruiter_reply_draft",
        ] {
            let name = template_name(kind);
            assert!(
                !name.is_empty() && name.len() <= MAX_TEMPLATE_NAME,
                "{kind}: {name}"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{kind}: {name}"
            );
            assert!(
                !name.starts_with('-') && !name.ends_with('-') && !name.contains("--"),
                "{kind}: {name}"
            );
        }
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

/// Against a real OpenShell gateway (CI's `gateway-contract` job starts one,
/// v0.1.6 with its Docker driver): what the fake runtime cannot check, such
/// as the gateway's own limits on names, templates and policies. Ignored
/// unless `OPENSHELL_CONTRACT_GATEWAY` names the gateway.
#[cfg(test)]
mod contract {
    use std::time::Duration;

    use url::Url;

    use super::*;
    use crate::{config::DEFAULT_AGENT_IMAGE, probe};

    #[tokio::test]
    #[ignore = "needs a real OpenShell gateway (OPENSHELL_CONTRACT_GATEWAY)"]
    async fn a_real_gateway_takes_our_templates_and_sandboxes() {
        let Ok(gateway) = std::env::var("OPENSHELL_CONTRACT_GATEWAY") else {
            return;
        };
        let image = std::env::var("OPENSHELL_CONTRACT_IMAGE")
            .unwrap_or_else(|_| DEFAULT_AGENT_IMAGE.to_owned());
        let runtime = OpenShellRuntime::anonymous(gateway, image.clone());
        let kinds = Kinds::new([probe::kind()]);

        // Every kind's template is taken, and taken again unchanged: the
        // gateway gives back the template we compare against, so a server
        // start does not replace it.
        runtime
            .prepare(&kinds)
            .await
            .expect("the templates are taken");
        let client = runtime.client().await.unwrap();
        let there = client
            .get_sandbox_template(&template_name(probe::KIND))
            .await
            .expect("the probe's template is there");
        assert!(
            same_template(&there, &template(&probe::kind(), &image)),
            "the gateway's copy is the same template"
        );
        runtime.prepare(&kinds).await.expect("prepared again");

        // A run's sandbox, with its policy, labels and command, as a run
        // makes it. Its runner cannot reach the example agent API and ends.
        let run = Uuid::now_v7();
        let handle = runtime
            .create(SandboxSpec {
                run,
                kind: probe::KIND,
                image,
                policy: probe::POLICY,
                api: Url::parse("https://applicamp.example.com/").unwrap(),
                token: "contract-test-token".to_owned(),
            })
            .await
            .expect("the run's sandbox is taken");
        // It is found by its labels, and kept while its run goes.
        assert_eq!(runtime.sweep(&HashSet::from([run])).await.unwrap(), 0);
        let exit = runtime
            .wait(&handle, Instant::now() + Duration::from_secs(120))
            .await
            .expect("the sandbox can be watched");
        eprintln!("the contract sandbox ended: {exit:?}");
        runtime
            .delete(&handle)
            .await
            .expect("the sandbox is deleted");
        // The gateway deletes in the background: it is listed a little while
        // longer (a sweep then deletes it again, harmlessly), then gone.
        let gone_by = Instant::now() + Duration::from_secs(60);
        loop {
            let ours = client
                .list_all_sandboxes(ListOptions {
                    label_selector: Some(format!("{RUN_LABEL}={run}")),
                    ..ListOptions::default()
                })
                .await
                .expect("the sandboxes are listed");
            if ours.is_empty() {
                break;
            }
            assert!(
                Instant::now() < gone_by,
                "the sandbox is still there after a minute"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
