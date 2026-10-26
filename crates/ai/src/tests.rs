//! The gateway against a fake OpenAI-compatible provider (wiremock) and a
//! real ledger (`#[sqlx::test]`).

use std::time::Duration;

use crate::{
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
        ..AiConfig::default()
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
    assert_eq!(body["usage"]["include"], true, "the cost is asked for");
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

fn completion_costing(content: &str, cost: f64) -> serde_json::Value {
    let mut body = completion(content);
    body["usage"]["cost"] = json!(cost);
    body
}

async fn spent_usd(db: &PgPool, account: Uuid) -> f64 {
    sqlx::query_scalar(
        "SELECT coalesce(sum(cost_usd), 0)::float8 FROM ai.usage WHERE account_id = $1",
    )
    .bind(account)
    .fetch_one(db)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_providers_cost_is_recorded_and_returned(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion_costing("{}", 0.000_42)))
        .mount(&server)
        .await;
    let account = Uuid::now_v7();
    let now = Utc::now();
    let gateway = gateway(config(&server, None));
    let answer = gateway
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
    assert_eq!(answer.cost_usd, Some(0.000_42));
    assert!((spent_usd(&db, account).await - 0.000_42).abs() < 1e-12);
    let spend = gateway.spend(&db, account, now).await.unwrap();
    assert!(
        (spend.month_eur - 0.000_42 * 0.92).abs() < 1e-9,
        "{spend:?}"
    );
    assert_eq!(spend.month_limit_eur, Some(3.0));
    let instance = gateway.instance_spend(&db, now).await.unwrap();
    assert_eq!(instance.calls, 1);
    assert_eq!(instance.limit_eur, Some(15.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_cost_the_price_table_counts(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion("{}")))
        .mount(&server)
        .await;
    let mut config = config(&server, None);
    if let Some(provider) = config.complete.take() {
        config.complete = Some(provider.with_price(1.0, 10.0));
    }
    let account = Uuid::now_v7();
    let answer = gateway(config)
        .complete(
            Caller {
                db: &db,
                account,
                now: Utc::now(),
            },
            request(),
        )
        .await
        .unwrap();
    // 120 tokens in at $1/M, 30 out at $10/M.
    let expected = (120.0 + 300.0) / 1_000_000.0;
    assert!((answer.cost_usd.unwrap() - expected).abs() < 1e-12);
    assert!((spent_usd(&db, account).await - expected).abs() < 1e-12);
}

#[sqlx::test(migrations = "../../migrations")]
async fn over_a_spending_limit_nothing_is_sent(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion_costing("{}", 0.05)))
        .mount(&server)
        .await;
    let mut config = config(&server, None);
    config.budget.account_monthly_eur = Some(0.10);
    config.budget.usd_to_eur = Some(1.0);
    if let Some(provider) = config.complete.take() {
        // Worst case of `request()`: a few tokens in, 100 out at $100/M = $0.01.
        config.complete = Some(provider.with_price(1.0, 100.0));
    }
    let gateway = gateway(config);
    let account = Uuid::now_v7();
    let now = Utc::now();
    let caller = Caller {
        db: &db,
        account,
        now,
    };
    gateway.complete(caller, request()).await.unwrap();
    gateway.complete(caller, request()).await.unwrap();
    // $0.10 spent: the next one's worst case would cross the month's limit.
    let err = gateway.complete(caller, request()).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::OverBudget {
                limit: crate::usage::Limit::AccountMonth
            }
        ),
        "{err}"
    );
    assert_eq!(
        applicamp_kernel::Error::from(err).code(),
        "ai_budget_reached"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    // Another account still has its own month, under the instance's limit.
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
async fn a_refused_call_costs_nothing(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    let account = Uuid::now_v7();
    gateway(config(&server, None))
        .complete(
            Caller {
                db: &db,
                account,
                now: Utc::now(),
            },
            request(),
        )
        .await
        .unwrap_err();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM ai.usage WHERE account_id = $1")
        .bind(account)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(rows, 0, "the reservation is released");
}
