//! Run kinds: what a module declares for each kind of agent run it owns
//! (ADR 0009).
//!
//! A kind is declared by the module that owns its subject (a company's
//! research belongs to the pipeline, a probe to `runs`): its image, its
//! network policy, its limits, the tools the agent may call (each tagged
//! read, draft or outward, with a call cap, an argument schema and the
//! owning module's handler) and the schema of the summary it finishes with.
//! The `runs` module only executes kinds; no module depends on it. A module
//! starts a run through a [`RunStarter`] port the server implements, and
//! hears the end by consuming [`RunFinished`].

use std::{collections::HashSet, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{BoxFuture, DomainEvent, Error, Result};

/// The sandbox image a kind runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Image {
    /// `applicamp-agent`: the runner alone.
    Agent,
    /// The runner with a browser (forms, pages that need scripts).
    Browser,
}

impl Image {
    /// As shown and stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Browser => "browser",
        }
    }
}

/// How far one run of a kind may go before it is ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Model calls and tool calls, together.
    pub max_steps: u32,
    /// From start to the deadline; past it the sandbox is removed and the
    /// run ends `ended_timeout`.
    pub wall_clock: Duration,
    /// Model tokens (in and out) the run may use.
    pub max_tokens: u32,
}

/// What a tool does to the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolClass {
    /// Reads what the account may see.
    Read,
    /// Writes a draft the person reviews.
    Draft,
    /// Leaves the person's name (an application, an email): it may only
    /// create an approval.
    Outward,
}

impl ToolClass {
    /// As shown and stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::Outward => "outward",
        }
    }
}

/// Who a tool call is for: the run, its account and subject.
#[derive(Debug, Clone, Copy)]
pub struct ToolContext<'a> {
    /// The database.
    pub db: &'a PgPool,
    /// The run calling.
    pub run: Uuid,
    /// The account it runs for.
    pub account: Uuid,
    /// What it is about (an opportunity, a company), if anything.
    pub subject: Option<Uuid>,
    /// The time.
    pub now: DateTime<Utc>,
}

/// Why a tool call gave no result.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The handler refuses these arguments: a code and what is wrong. The
    /// agent sees both.
    #[error("refused: {0}")]
    Refused(&'static str, String),
    /// Anything else, as a handler error.
    #[error(transparent)]
    Failed(#[from] Error),
}

impl From<sqlx::Error> for ToolError {
    fn from(err: sqlx::Error) -> Self {
        Self::Failed(err.into())
    }
}

impl From<ToolError> for Error {
    fn from(err: ToolError) -> Self {
        match err {
            ToolError::Refused(code, detail) => Self::BadRequest(code, detail),
            ToolError::Failed(err) => err,
        }
    }
}

/// A module's handler of one tool. Arguments arrive checked against the
/// tool's schema; the result goes back to the agent as is.
pub trait ToolHandler: Send + Sync + 'static {
    /// Handle one call.
    fn call<'a>(
        &'a self,
        ctx: ToolContext<'a>,
        args: Value,
    ) -> BoxFuture<'a, Result<Value, ToolError>>;
}

/// A tool a kind's agent may call.
#[derive(Clone)]
pub struct ToolSpec {
    /// Its name, unique within the kind (`probe_echo`).
    pub name: &'static str,
    /// What it does to the world.
    pub class: ToolClass,
    /// Calls one run may make.
    pub cap: u32,
    /// The JSON schema its arguments match.
    pub schema: Value,
    /// The owning module's handler.
    pub handler: Arc<dyn ToolHandler>,
}

impl std::fmt::Debug for ToolSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSpec")
            .field("name", &self.name)
            .field("class", &self.class)
            .field("cap", &self.cap)
            .finish_non_exhaustive()
    }
}

/// A kind of agent run, as its owning module declares it.
#[derive(Debug, Clone)]
pub struct RunKind {
    /// `<module>.<kind>`, e.g. `runs.probe`; never rename it.
    pub name: &'static str,
    /// The image it runs in.
    pub image: Image,
    /// Its network policy: the YAML of `policies/<kind>.yaml`, embedded
    /// (OpenShell policy v1).
    pub policy: &'static str,
    /// How far a run may go.
    pub limits: Limits,
    /// The tools its agent may call.
    pub tools: Vec<ToolSpec>,
    /// The JSON schema of the summary a run finishes with.
    pub output: Value,
    /// A person may start one from the Agents page.
    pub manual: bool,
}

impl RunKind {
    /// The module that declares it: the part of the name before the dot.
    pub fn module(&self) -> &'static str {
        self.name
            .split_once('.')
            .map_or(self.name, |(module, _)| module)
    }

    /// The tool named `name`, if the kind has it.
    pub fn tool(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}

/// Every module's kinds (`(module, kinds)`) are well formed: named
/// `<module>.<kind>` after the module declaring them and unique, with a
/// policy, a deadline, tools unique within the kind with a cap of at least
/// one and an object schema, and no outward tool (an outward tool may only
/// create an approval, and the approvals module does not exist yet).
pub fn check_kinds<'a>(
    modules: impl IntoIterator<Item = (&'a str, &'a [RunKind])>,
) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let mut names = HashSet::new();
    for (module, kinds) in modules {
        for kind in kinds {
            let name = kind.name;
            if !name
                .strip_prefix(module)
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('.'))
            {
                problems.push(format!(
                    "run kind `{name}` (module `{module}`) must be named `{module}.<kind>`"
                ));
            }
            if !names.insert(name) {
                problems.push(format!("run kind `{name}` is declared twice"));
            }
            if kind.policy.trim().is_empty() {
                problems.push(format!("run kind `{name}` has no policy"));
            }
            if kind.limits.wall_clock.is_zero() || kind.limits.max_steps == 0 {
                problems.push(format!(
                    "run kind `{name}` needs a wall clock and at least one step"
                ));
            }
            if !kind.output.is_object() {
                problems.push(format!(
                    "run kind `{name}`: its output schema is not an object"
                ));
            }
            let mut tools = HashSet::new();
            for tool in &kind.tools {
                if !tools.insert(tool.name) {
                    problems.push(format!(
                        "run kind `{name}`: tool `{}` is declared twice",
                        tool.name
                    ));
                }
                if tool.cap == 0 {
                    problems.push(format!(
                        "run kind `{name}`: tool `{}` has a cap of 0",
                        tool.name
                    ));
                }
                if !tool.schema.is_object() {
                    problems.push(format!(
                        "run kind `{name}`: tool `{}` has no object schema",
                        tool.name
                    ));
                }
                if tool.class == ToolClass::Outward {
                    problems.push(format!(
                        "run kind `{name}`: tool `{}` is outward, and outward tools may only \
                         create an approval (the approvals module does not exist yet)",
                        tool.name
                    ));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        problems.sort();
        Err(problems)
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// It did what it was for.
    Succeeded,
    /// It looked and found nothing new.
    NothingNew,
    /// It needs the person (a form only a human may fill).
    NeedsYou,
    /// It reached its kind's step limit.
    EndedMaxSteps,
    /// It reached its kind's token budget, or the account's.
    EndedBudget,
    /// It was still going at its deadline.
    EndedTimeout,
    /// It kept doing the same thing.
    EndedLoop,
    /// It failed (the agent said so, or its sandbox did).
    Failed,
    /// Someone stopped it.
    Cancelled,
}

impl Outcome {
    /// Every outcome.
    pub const ALL: [Self; 9] = [
        Self::Succeeded,
        Self::NothingNew,
        Self::NeedsYou,
        Self::EndedMaxSteps,
        Self::EndedBudget,
        Self::EndedTimeout,
        Self::EndedLoop,
        Self::Failed,
        Self::Cancelled,
    ];

    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::NothingNew => "nothing_new",
            Self::NeedsYou => "needs_you",
            Self::EndedMaxSteps => "ended_max_steps",
            Self::EndedBudget => "ended_budget",
            Self::EndedTimeout => "ended_timeout",
            Self::EndedLoop => "ended_loop",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// The outcome stored as `raw`.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|outcome| outcome.as_str() == raw)
    }
}

/// A run to start, in the caller's transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct StartRun {
    /// The kind (`<module>.<kind>`).
    pub kind: String,
    /// The account it runs for.
    pub account: Uuid,
    /// What it is about, if anything.
    pub subject: Option<Uuid>,
    /// Its inputs (ids and settings, never personal data).
    pub inputs: Value,
}

/// The port a module starts runs through; the server implements it over
/// the `runs` module.
pub trait RunStarter: Send + Sync {
    /// Queue `run` in `conn`'s transaction: it starts once that commits. The
    /// run's id.
    fn start_run<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        run: StartRun,
    ) -> BoxFuture<'a, Result<Uuid>>;
}

/// A run ended. Ids and the outcome only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinished {
    /// The run.
    pub run: Uuid,
    /// Its account.
    pub account: Uuid,
    /// Its kind (`<module>.<kind>`).
    pub kind: String,
    /// What it was about, if anything.
    pub subject: Option<Uuid>,
    /// How it ended.
    pub outcome: Outcome,
}

impl DomainEvent for RunFinished {
    const NAME: &'static str = "runs.finished";
    const VERSION: u16 = 1;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct Echo;

    impl ToolHandler for Echo {
        fn call<'a>(
            &'a self,
            _ctx: ToolContext<'a>,
            args: Value,
        ) -> BoxFuture<'a, Result<Value, ToolError>> {
            Box::pin(async move { Ok(args) })
        }
    }

    fn tool(name: &'static str, class: ToolClass, cap: u32) -> ToolSpec {
        ToolSpec {
            name,
            class,
            cap,
            schema: json!({ "type": "object" }),
            handler: Arc::new(Echo),
        }
    }

    fn kind(name: &'static str, tools: Vec<ToolSpec>) -> RunKind {
        RunKind {
            name,
            image: Image::Agent,
            policy: "version: 1\n",
            limits: Limits {
                max_steps: 4,
                wall_clock: Duration::from_secs(60),
                max_tokens: 0,
            },
            tools,
            output: json!({ "type": "object" }),
            manual: false,
        }
    }

    #[test]
    fn well_formed_kinds_pass() {
        let kinds = [kind(
            "runs.probe",
            vec![tool("probe_echo", ToolClass::Read, 1)],
        )];
        assert!(check_kinds([("runs", kinds.as_slice())]).is_ok());
        assert_eq!(kinds[0].module(), "runs");
        assert!(kinds[0].tool("probe_echo").is_some());
        assert!(kinds[0].tool("other").is_none());
    }

    #[test]
    fn misnamed_duplicated_and_outward_kinds_fail() {
        let runs = [
            kind(
                "runs.probe",
                vec![
                    tool("a", ToolClass::Outward, 1),
                    tool("a", ToolClass::Read, 0),
                ],
            ),
            kind("runs.probe", vec![]),
        ];
        let pipeline = [kind("runs.research", vec![])];
        let problems = check_kinds([("runs", runs.as_slice()), ("pipeline", pipeline.as_slice())])
            .unwrap_err();
        for expected in [
            "must be named `pipeline.",
            "declared twice",
            "is outward",
            "cap of 0",
        ] {
            assert!(
                problems.iter().any(|p| p.contains(expected)),
                "{expected}: {problems:?}"
            );
        }
    }

    #[test]
    fn outcomes_round_trip() {
        for outcome in Outcome::ALL {
            assert_eq!(Outcome::parse(outcome.as_str()), Some(outcome));
            assert_eq!(
                serde_json::to_value(outcome).unwrap(),
                json!(outcome.as_str())
            );
        }
        assert_eq!(Outcome::parse("nope"), None);
    }

    #[test]
    fn tool_errors_become_problems() {
        let refused: Error =
            ToolError::Refused("too_long", "At most 200 characters.".into()).into();
        assert_eq!(refused.code(), "too_long");
        assert_eq!(refused.status().as_u16(), 400);
    }
}
