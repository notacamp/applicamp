use std::{sync::Arc, time::Duration};

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    RunsDeps, probe,
    store::{Layer, Level},
    testkit::{Harness, kind, scripted},
};

fn harness(db: PgPool) -> Harness {
    Harness::new(db, scripted("true"))
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_context_names_the_run_its_limits_and_its_tools(db: PgPool) {
    let harness = harness(db);
    let (run, token) = harness.going(harness.account, &probe::kind()).await;
    let (status, body) = harness.agent("GET", "context", &token, None).await;
    assert_eq!(status.as_u16(), 200, "{body}");
    assert_eq!(body["run"], run.to_string());
    assert_eq!(body["kind"], probe::KIND);
    assert_eq!(body["limits"]["wall_clock_secs"], 120);
    assert_eq!(body["limits"]["max_tokens"], 0);
    assert_eq!(body["tools"][0]["name"], probe::ECHO);
    assert_eq!(body["tools"][0]["class"], "read");
    assert_eq!(body["tools"][0]["cap"], 1);
    assert_eq!(body["tools"][0]["schema"]["required"], json!(["text"]));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_token_works_only_for_its_own_going_run(db: PgPool) {
    let harness = harness(db);
    let (a, token_a) = harness.going(harness.account, &probe::kind()).await;
    let (b, token_b) = harness.going(Uuid::now_v7(), &probe::kind()).await;

    // Each token is its own run's; an unknown one is nobody's.
    let (_, body) = harness.agent("GET", "context", &token_b, None).await;
    assert_eq!(body["run"], b.to_string());
    let (status, body) = harness.agent("GET", "context", "not-a-token", None).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (404, Some("not_found"))
    );

    // Run A finishes: its token is gone, and B's still works.
    let finished = json!({ "outcome": "succeeded", "summary": { "checks": [] } });
    let (status, _) = harness
        .agent("POST", "finish", &token_a, Some(&finished))
        .await;
    assert_eq!(status.as_u16(), 202);
    let (status, _) = harness.agent("GET", "context", &token_a, None).await;
    assert_eq!(status.as_u16(), 404, "after finish");
    crate::store::finish(
        &harness.db,
        &harness.state.events,
        a,
        applicamp_kernel::agent::Outcome::Succeeded,
        None,
        Utc::now(),
    )
    .await
    .unwrap();
    let (status, _) = harness
        .agent("POST", "finish", &token_a, Some(&finished))
        .await;
    assert_eq!(status.as_u16(), 404, "a finished run");
    assert!(
        harness.run(b).await.outcome.is_none(),
        "nothing changed for B"
    );
    let (status, _) = harness.agent("GET", "context", &token_b, None).await;
    assert_eq!(status.as_u16(), 200);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_expired_token_is_not_found(db: PgPool) {
    let harness = harness(db);
    let (run, token) = harness.going(harness.account, &probe::kind()).await;
    sqlx::query(
        "UPDATE runs.tokens SET expires_at = now() - interval '1 second' WHERE run_id = $1",
    )
    .bind(run)
    .execute(&harness.db)
    .await
    .unwrap();
    let (status, _) = harness.agent("GET", "context", &token, None).await;
    assert_eq!(status.as_u16(), 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_cookie_or_no_token_is_refused(db: PgPool) {
    let harness = harness(db);
    let (_, token) = harness.going(harness.account, &probe::kind()).await;
    let (status, body) = harness.agent_with_cookie("context", &token).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (400, Some("cookies_refused"))
    );
    let (status, body) = harness.agent("GET", "context", "", None).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (401, Some("run_token_required"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn tools_are_the_kinds_within_their_caps_and_schemas(db: PgPool) {
    let harness = harness(db);
    let (_, token) = harness.going(harness.account, &probe::kind()).await;

    let (status, _) = harness
        .agent(
            "POST",
            "tools/send_email",
            &token,
            Some(&json!({ "args": {} })),
        )
        .await;
    assert_eq!(status.as_u16(), 404, "not the kind's tool");

    let (status, body) = harness
        .agent(
            "POST",
            "tools/probe_echo",
            &token,
            Some(&json!({ "args": { "text": 7 } })),
        )
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (400, Some("invalid_arguments"))
    );
    assert!(
        body["detail"].as_str().unwrap().starts_with("$.text"),
        "{body}"
    );

    // Bad arguments do not count against the cap.
    let (status, body) = harness
        .agent(
            "POST",
            "tools/probe_echo",
            &token,
            Some(&json!({ "args": { "text": "hi" } })),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "{body}");
    assert_eq!(body["result"]["echo"], "hi");

    let (status, body) = harness
        .agent(
            "POST",
            "tools/probe_echo",
            &token,
            Some(&json!({ "args": { "text": "hi" } })),
        )
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (409, Some("tool_cap_reached"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn events_are_kept_once_and_bounded(db: PgPool) {
    let harness = harness(db);
    let (run, token) = harness.going(harness.account, &probe::kind()).await;
    let line = json!([{ "seq": 1, "level": "info", "message": "Read its context.", "operation": "context" }]);
    let (status, body) = harness.agent("POST", "events", &token, Some(&line)).await;
    assert_eq!((status.as_u16(), body["recorded"].as_u64()), (202, Some(1)));
    let (_, body) = harness.agent("POST", "events", &token, Some(&line)).await;
    assert_eq!(body["recorded"], 0, "the same line twice is kept once");

    let long = json!([{ "seq": 2, "level": "info", "message": "x".repeat(2001) }]);
    let (status, body) = harness.agent("POST", "events", &token, Some(&long)).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (400, Some("invalid_event"))
    );
    let many: Vec<Value> = (1..=51)
        .map(|seq| json!({ "seq": seq, "level": "info", "message": "x" }))
        .collect();
    let (status, _) = harness
        .agent("POST", "events", &token, Some(&json!(many)))
        .await;
    assert_eq!(status.as_u16(), 400);
    let (status, _) = harness
        .agent(
            "POST",
            "events",
            &token,
            Some(&json!([{ "seq": 3, "level": "shout", "message": "x" }])),
        )
        .await;
    assert_eq!(status.as_u16(), 400);

    let events = harness.events(run).await;
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].layer, events[0].level),
        (Layer::Agent, Level::Info)
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_run_makes_at_most_ten_calls_a_second(db: PgPool) {
    let harness = harness(db);
    let (_, token) = harness.going(harness.account, &probe::kind()).await;
    for _ in 0..10 {
        let (status, _) = harness.agent("GET", "context", &token, None).await;
        assert_eq!(status.as_u16(), 200);
    }
    let (status, body) = harness.agent("GET", "context", &token, None).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (429, Some("rate_limited"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn finish_checks_the_summary_against_the_kind(db: PgPool) {
    let harness = harness(db);
    let (run, token) = harness.going(harness.account, &probe::kind()).await;
    let (status, body) = harness
        .agent(
            "POST",
            "finish",
            &token,
            Some(&json!({ "outcome": "succeeded", "summary": { "checks": [1] } })),
        )
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (400, Some("invalid_summary"))
    );
    let (status, _) = harness
        .agent(
            "POST",
            "finish",
            &token,
            Some(&json!({ "outcome": "ended_timeout", "summary": {} })),
        )
        .await;
    assert_eq!(status.as_u16(), 400, "only the server ends a run on time");
    let (status, _) = harness
        .agent(
            "POST",
            "finish",
            &token,
            Some(&json!({ "outcome": "needs_you", "summary": { "checks": ["context"] } })),
        )
        .await;
    assert_eq!(status.as_u16(), 202);
    let row = harness.run(run).await;
    assert_eq!(row.outcome.as_deref(), Some("needs_you"));
    assert_eq!(
        row.status, "running",
        "the server ends it once its sandbox is gone"
    );
}

/// A fake OpenAI-compatible model answering every call with 900 tokens in
/// and 300 out.
async fn model() -> wiremock::MockServer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = wiremock::MockServer::builder()
        .listener(listener)
        .start()
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "model": "test/model",
            "choices": [{ "message": { "role": "assistant", "content": "{\"ok\":true}" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 900, "completion_tokens": 300 },
        })))
        .mount(&server)
        .await;
    server
}

fn gateway(server: &wiremock::MockServer, daily_tokens: u64) -> applicamp_ai::Gateway {
    use applicamp_ai::{AiConfig, ProviderConfig, RetryPolicy};
    applicamp_ai::Gateway::new(AiConfig {
        complete: Some(
            ProviderConfig::new(
                format!("{}/v1", server.uri()).parse().unwrap(),
                "test/model",
            )
            .with_price(0.15, 0.5),
        ),
        daily_tokens: Some(daily_tokens),
        ..AiConfig::default()
    })
    .unwrap()
    .with_retry(RetryPolicy {
        attempts: 1,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(10),
    })
}

fn ask(max_tokens: u32) -> Value {
    json!({
        "messages": [{ "role": "system", "content": "Answer in JSON." }, { "role": "user", "content": "ok?" }],
        "response_format": { "type": "json_object" },
        "max_tokens": max_tokens,
    })
}

#[sqlx::test(migrations = "../../../migrations")]
async fn complete_counts_the_run_and_holds_to_the_caps(db: PgPool) {
    let server = model().await;
    let thinker = kind("runs.thinker", Duration::from_secs(60), 3_000);
    let deps = RunsDeps::new(crate::RunsConfig::default(), Some(gateway(&server, 1_000)))
        .with_kinds([thinker.clone()])
        .with_runtime(
            scripted("true"),
            url::Url::parse("http://127.0.0.1:9").unwrap(),
        );
    let harness = Harness::with_deps(db, deps);
    let (run, token) = harness.going(harness.account, &thinker).await;

    let (status, body) = harness
        .agent("POST", "complete", &token, Some(&ask(500)))
        .await;
    assert_eq!(status.as_u16(), 200, "{body}");
    assert_eq!(body["content"], "{\"ok\":true}");
    assert_eq!(
        (body["tokens_in"].as_u64(), body["tokens_out"].as_u64()),
        (Some(900), Some(300))
    );
    let row = harness.run(run).await;
    assert_eq!((row.tokens_in, row.tokens_out), (900, 300));
    assert!(row.cost_eur > 0.0);
    assert_eq!(row.model.as_deref(), Some("test/model"));

    // The account's daily tokens are used up: refused, and the run says so.
    let (status, body) = harness
        .agent("POST", "complete", &token, Some(&ask(500)))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (503, Some("ai_daily_limit_reached"))
    );
    let events = harness.events(run).await;
    assert!(
        events
            .iter()
            .any(|e| e.level == Level::Error && e.message.contains("ai_daily_limit_reached")),
        "{events:?}"
    );

    // Past the kind's budget: refused before the model is asked.
    let (status, body) = harness
        .agent("POST", "complete", &token, Some(&ask(2_000)))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (429, Some("run_budget_reached"))
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_probe_has_no_token_budget(db: PgPool) {
    let server = model().await;
    let deps = RunsDeps::new(
        crate::RunsConfig::default(),
        Some(gateway(&server, 100_000)),
    )
    .with_runtime(
        Arc::new(crate::runtime::fake::FakeRuntime::new("unused".into())),
        url::Url::parse("http://127.0.0.1:9").unwrap(),
    );
    let harness = Harness::with_deps(db, deps);
    let (_, token) = harness.going(harness.account, &probe::kind()).await;
    let (status, body) = harness
        .agent("POST", "complete", &token, Some(&ask(10)))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (429, Some("run_budget_reached"))
    );
    let (status, _) = harness
        .agent("POST", "complete", &token, Some(&ask(20_000)))
        .await;
    assert_eq!(status.as_u16(), 400, "max_tokens is at most 16 000");
    assert!(server.received_requests().await.unwrap().is_empty());
}
