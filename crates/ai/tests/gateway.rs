//! The gateway against a fake OpenAI-compatible provider (wiremock) and a
//! real ledger (`#[sqlx::test]`).

use std::time::Duration;

use applicamp_ai::{
    AiConfig, Caller, ChatRequest, Error, Gateway, Message, ProviderConfig, ResponseFormat,
    RetryPolicy, usage,
};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

fn config(server: &MockServer, daily_tokens: Option<u64>) -> AiConfig {
    AiConfig {
        complete: Some(
            ProviderConfig::new(
                format!("{}/api/v1", server.uri()).parse().unwrap(),
                "test/model",
            )
            .with_api_key("sk-test"),
        ),
        daily_tokens,
    }
}

fn gateway(config: AiConfig) -> Gateway {
    Gateway::new(config).unwrap().with_retry(RetryPolicy {
        attempts: 3,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(20),
    })
}

fn completion(content: &str) -> serde_json::Value {
    json!({
        "model": "test/model-2026",
        "choices": [{ "message": { "role": "assistant", "content": content },
                      "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 120, "completion_tokens": 30 },
    })
}

fn request() -> ChatRequest {
    ChatRequest {
        temperature: Some(0.0),
        max_tokens: Some(100),
        response_format: Some(ResponseFormat::json_schema(
            "answer",
            json!({ "type": "object" }),
        )),
        ..ChatRequest::new(vec![
            Message::system("Answer in JSON."),
            Message::user("Hi"),
        ])
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_busy_provider_is_retried_and_the_call_is_counted(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion(r#"{"ok": true}"#)))
        .mount(&server)
        .await;
    let account = Uuid::now_v7();
    let now = Utc::now();

    let answer = gateway(config(&server, None))
        .complete(
            Caller {
                db: &db,
                account,
                now,
            },
            request(),
        )
        .await
        .unwrap();
    assert_eq!(
        answer.json::<serde_json::Value>().unwrap(),
        json!({ "ok": true })
    );
    assert_eq!(answer.model, "test/model-2026");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(body["model"], "test/model");
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["name"], "answer");

    assert_eq!(usage::used_today(&db, account, now).await.unwrap(), 150);
    let (role, model): (String, String) =
        sqlx::query_as("SELECT role, model FROM ai.usage WHERE account_id = $1")
            .bind(account)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        (role.as_str(), model.as_str()),
        ("complete", "test/model-2026")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_provider_fails_after_the_attempts(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(502).set_body_string("upstream echoed: Hi"))
        .mount(&server)
        .await;
    let err = gateway(config(&server, None))
        .complete(
            Caller {
                db: &db,
                account: Uuid::now_v7(),
                now: Utc::now(),
            },
            request(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Status { status: 502 }), "{err}");
    assert!(!err.to_string().contains("echoed"));
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_client_error_is_not_retried(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    let err = gateway(config(&server, None))
        .complete(
            Caller {
                db: &db,
                account: Uuid::now_v7(),
                now: Utc::now(),
            },
            request(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Status { status: 400 }), "{err}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn over_the_daily_cap_nothing_is_sent(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion("{}")))
        .mount(&server)
        .await;
    let account = Uuid::now_v7();
    let now = Utc::now();
    let gateway = gateway(config(&server, Some(200)));

    // 150 tokens: under the cap of 200, so the next call goes out…
    gateway
        .complete(
            Caller {
                db: &db,
                account,
                now,
            },
            request(),
        )
        .await
        .unwrap();
    // …and brings the day to 300: the one after is refused.
    gateway
        .complete(
            Caller {
                db: &db,
                account,
                now,
            },
            request(),
        )
        .await
        .unwrap();
    let err = gateway
        .complete(
            Caller {
                db: &db,
                account,
                now,
            },
            request(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::OverDailyCap { cap: 200 }), "{err}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // Another account has its own day.
    gateway
        .complete(
            Caller {
                db: &db,
                account: Uuid::now_v7(),
                now,
            },
            request(),
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unconfigured_role_sends_nothing(db: PgPool) {
    let err = gateway(AiConfig::default())
        .complete(
            Caller {
                db: &db,
                account: Uuid::now_v7(),
                now: Utc::now(),
            },
            request(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotConfigured(_)), "{err}");
    assert_eq!(applicamp_kernel::Error::from(err).code(), "ai_unavailable");
}
