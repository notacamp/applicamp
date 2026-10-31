//! `runs.probe`: the check that the harness works end to end ("Run a
//! check" on the Agents page).
//!
//! Its agent reads its context, calls `probe_echo` once, is refused a
//! second call past the tool's cap of one, checks that a host outside its
//! policy is blocked, and finishes `succeeded` (`crates/agent`). It makes no
//! model call, so its token budget is zero.

use std::{sync::Arc, time::Duration};

use applicamp_kernel::{
    BoxFuture,
    agent::{Image, Limits, RunKind, ToolClass, ToolContext, ToolError, ToolHandler, ToolSpec},
};
use serde_json::{Value, json};

/// The kind's name.
pub const KIND: &str = "runs.probe";
/// Its one tool.
pub const ECHO: &str = "probe_echo";
/// Its policy (`policies/probe.yaml`).
pub const POLICY: &str = include_str!("../../../../policies/probe.yaml");

/// `probe_echo`: answers the text it was given.
#[derive(Debug, Clone, Copy)]
pub struct Echo;

impl ToolHandler for Echo {
    fn call<'a>(
        &'a self,
        _ctx: ToolContext<'a>,
        args: Value,
    ) -> BoxFuture<'a, Result<Value, ToolError>> {
        Box::pin(async move {
            let text = args["text"].as_str().unwrap_or_default().to_owned();
            Ok(json!({ "echo": text }))
        })
    }
}

/// The probe kind.
pub fn kind() -> RunKind {
    RunKind {
        name: KIND,
        image: Image::Agent,
        policy: POLICY,
        limits: Limits {
            max_steps: 8,
            wall_clock: Duration::from_secs(120),
            max_tokens: 0,
        },
        tools: vec![ToolSpec {
            name: ECHO,
            class: ToolClass::Read,
            cap: 1,
            schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string", "maxLength": 200 } },
                "required": ["text"],
                "additionalProperties": false,
            }),
            handler: Arc::new(Echo),
        }],
        output: json!({
            "type": "object",
            "properties": {
                "checks": {
                    "type": "array",
                    "items": { "type": "string", "maxLength": 200 },
                    "maxItems": 10,
                },
            },
            "required": ["checks"],
            "additionalProperties": false,
        }),
        manual: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_is_well_formed_and_its_policy_parses() {
        let kind = kind();
        assert!(
            applicamp_kernel::agent::check_kinds([("runs", std::slice::from_ref(&kind))]).is_ok()
        );
        assert!(openshell_policy::parse_sandbox_policy(kind.policy).is_ok());
        assert!(crate::schema::check(&kind.output, &json!({ "checks": ["context"] })).is_ok());
    }
}
