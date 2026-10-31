//! `runs.probe`: the harness's own check, step by step.
//!
//! 1. read the run's context;
//! 2. call `probe_echo` and get its text back;
//! 3. call it again and be refused past its cap (`tool_cap_reached`);
//! 4. try a host outside the policy and be blocked;
//! 5. finish `succeeded` when every step went as expected, else `failed`,
//!    with the steps that passed as the summary.

use serde_json::{Value, json};
use url::Url;

use crate::api::Api;

/// The kind this module runs.
pub const KIND: &str = "runs.probe";
/// Its tool.
pub const ECHO: &str = "probe_echo";
/// What it echoes.
const TEXT: &str = "hello from the sandbox";
/// The host outside every policy it tries.
pub const OUTSIDE: &str = "https://example.com/";

/// Run the probe against `api` with the run's `context`, trying `outside`
/// as the host outside its policy. The outcome it finished with.
pub async fn run(api: &Api, context: &Value, outside: &Url) -> &'static str {
    let mut passed: Vec<&str> = Vec::new();
    let mut failed = false;

    if context["kind"] == KIND {
        api.event("info", "Read its context.", Some("context"), None)
            .await;
        passed.push("context");
    } else {
        api.event(
            "error",
            "Its context is not a probe's.",
            Some("context"),
            None,
        )
        .await;
        failed = true;
    }

    match api.tool(ECHO, json!({ "text": TEXT })).await {
        Ok(result) if result["echo"] == TEXT => {
            api.event("tool", "Called probe_echo; it answered.", None, Some(ECHO))
                .await;
            passed.push("tool");
        }
        Ok(_) => {
            api.event(
                "error",
                "probe_echo answered something else.",
                None,
                Some(ECHO),
            )
            .await;
            failed = true;
        }
        Err(e) => {
            api.event(
                "error",
                &format!("probe_echo failed: {e}."),
                None,
                Some(ECHO),
            )
            .await;
            failed = true;
        }
    }

    match api.tool(ECHO, json!({ "text": TEXT })).await {
        Err(e) if e.code() == Some("tool_cap_reached") => {
            api.event(
                "deny",
                "probe_echo was refused past its cap of one call.",
                None,
                Some(ECHO),
            )
            .await;
            passed.push("cap");
        }
        Ok(_) => {
            api.event(
                "error",
                "probe_echo answered past its cap.",
                None,
                Some(ECHO),
            )
            .await;
            failed = true;
        }
        Err(e) => {
            api.event(
                "error",
                &format!("The second probe_echo failed: {e}."),
                None,
                Some(ECHO),
            )
            .await;
            failed = true;
        }
    }

    let host = outside.host_str().unwrap_or("the host");
    match api.http().get(outside.clone()).send().await {
        Ok(response) if response.status().is_success() => {
            api.event(
                "error",
                &format!("Reached {host}, which is outside its policy."),
                Some("network"),
                None,
            )
            .await;
            failed = true;
        }
        Ok(_) | Err(_) => {
            api.event(
                "deny",
                &format!("{host} is blocked: it is outside its policy."),
                Some("network"),
                None,
            )
            .await;
            passed.push("network");
        }
    }

    let outcome = if failed { "failed" } else { "succeeded" };
    match api.finish(outcome, json!({ "checks": passed })).await {
        Ok(()) => outcome,
        Err(_) => "failed",
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;

    /// A fake agent API: `probe_echo` answers once, then is past its cap.
    struct Echo(std::sync::atomic::AtomicUsize);

    impl Respond for Echo {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(json!({
                    "result": { "echo": body["args"]["text"] },
                }))
            } else {
                ResponseTemplate::new(409).set_body_json(json!({ "code": "tool_cap_reached" }))
            }
        }
    }

    async fn api() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent/v1/tools/probe_echo"))
            .and(header("authorization", "Bearer t0ken"))
            .respond_with(Echo(std::sync::atomic::AtomicUsize::new(0)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/agent/v1/events"))
            .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "recorded": 1 })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/agent/v1/finish"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;
        server
    }

    async fn finished_with(server: &MockServer) -> Value {
        let requests = server.received_requests().await.unwrap();
        let finish = requests
            .iter()
            .find(|r| r.url.path() == "/api/agent/v1/finish")
            .unwrap();
        serde_json::from_slice(&finish.body).unwrap()
    }

    fn client(server: &MockServer) -> Api {
        let base = Url::parse(&server.uri()).unwrap();
        Api::new(reqwest::Client::new(), base, "t0ken".to_owned())
    }

    #[tokio::test]
    async fn every_step_passes_when_the_outside_host_is_blocked() {
        let server = api().await;
        // Nothing listens there: as blocked as a refused CONNECT.
        let outside = Url::parse("http://127.0.0.1:9/").unwrap();
        assert_eq!(
            run(&client(&server), &json!({ "kind": KIND }), &outside).await,
            "succeeded"
        );
        let finish = finished_with(&server).await;
        assert_eq!(finish["outcome"], "succeeded");
        assert_eq!(
            finish["summary"]["checks"],
            json!(["context", "tool", "cap", "network"])
        );
        let events = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/api/agent/v1/events")
            .count();
        assert_eq!(events, 4);
    }

    #[tokio::test]
    async fn reaching_the_outside_host_fails_the_probe() {
        let server = api().await;
        let open = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&open)
            .await;
        let outside = Url::parse(&open.uri()).unwrap();
        assert_eq!(
            run(&client(&server), &json!({ "kind": KIND }), &outside).await,
            "failed"
        );
        let finish = finished_with(&server).await;
        assert_eq!(finish["outcome"], "failed");
        assert_eq!(
            finish["summary"]["checks"],
            json!(["context", "tool", "cap"])
        );
    }
}
