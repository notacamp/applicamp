//! Goals over HTTP: reading, saving, versions, limits, choices, currencies.

use serde_json::{Value, json};
use sqlx::PgPool;

use crate::testkit::Harness;

#[sqlx::test(migrations = "../../../migrations")]
async fn no_goals_yet_then_answers_saved_as_version_one(db: PgPool) {
    let h = Harness::new(db, None);
    let (status, view) = h.get("/api/v1/goals").await;
    assert_eq!(status, 200);
    assert_eq!(view["version"], 0);
    assert_eq!(view["updated_at"], Value::Null);
    assert_eq!(view["answers"]["roles"], json!([]));
    assert!(view["answers"].get("cash_floor").is_none());

    let (status, saved) = h
        .json(
            "PUT",
            "/api/v1/goals",
            &json!({
                "roles": ["  Staff engineer ", "", "staff   ENGINEER", "Engineering manager"],
                "seniority": ["staff", "staff", "lead"],
                "work_mode": ["remote", "hybrid"],
                "locations": ["Lisbon"],
                "relocate": "within_eu",
                "cash_floor": { "amount": 90000, "currency": " eur ", "period": "year" },
                "equity_counts": false,
                "company_stage": ["growth"],
                "languages_for_work": ["en", "pt-PT"],
                "autonomy": "prepare",
            }),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["version"], 1);
    let answers = &saved["answers"];
    assert_eq!(
        answers["roles"],
        json!(["Staff engineer", "Engineering manager"])
    );
    assert_eq!(answers["seniority"], json!(["staff", "lead"]));
    assert_eq!(answers["cash_floor"]["currency"], "EUR");
    assert_eq!(answers["equity_counts"], false);
    assert_eq!(answers["avoid"], json!([]));

    let (_, again) = h.get("/api/v1/goals").await;
    assert_eq!(again["version"], 1);
    assert_eq!(again["answers"], *answers);
    assert!(again["updated_at"].is_string());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn every_save_is_a_new_version_and_a_skipped_question_is_unanswered(db: PgPool) {
    let h = Harness::new(db, None);
    let floor = |amount: u32| json!({ "cash_floor": { "amount": amount, "currency": "EUR", "period": "year" } });
    let (_, first) = h.json("PUT", "/api/v1/goals", &floor(90_000)).await;
    assert_eq!(first["version"], 1);

    let (_, second) = h.json("PUT", "/api/v1/goals", &floor(95_000)).await;
    assert_eq!(second["version"], 2);
    assert_eq!(second["answers"]["cash_floor"]["amount"], 95_000);

    // The same answers again is still a new version.
    let (_, third) = h.json("PUT", "/api/v1/goals", &floor(95_000)).await;
    assert_eq!(third["version"], 3);

    // Leaving the question out clears it.
    let (_, fourth) = h.json("PUT", "/api/v1/goals", &json!({})).await;
    assert_eq!(fourth["version"], 4);
    assert!(fourth["answers"].get("cash_floor").is_none());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn goals_are_private_and_publish_their_version(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    h.json("PUT", "/api/v1/goals", &json!({ "roles": ["Designer"] }))
        .await;
    h.json(
        "PUT",
        "/api/v1/goals",
        &json!({ "roles": ["Designer", "Writer"] }),
    )
    .await;

    let (_, theirs) = Harness::new(db.clone(), None).get("/api/v1/goals").await;
    assert_eq!(theirs["version"], 0);
    assert_eq!(theirs["answers"]["roles"], json!([]));

    let payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM kernel.events WHERE name = 'profile.goals_updated'
         ORDER BY (payload->>'version')::int",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[1]["version"], 2);
    assert_eq!(payloads[1]["account"], h.account.to_string());
    assert!(
        !payloads[1].to_string().contains("Writer"),
        "answers never travel in events"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn answers_that_do_not_fit_are_refused(db: PgPool) {
    let h = Harness::new(db, None);
    let long = "x".repeat(101);
    let eleven: Vec<String> = (0..11).map(|n| format!("item {n}")).collect();
    let floor = |amount: u64, currency: &str, period: &str| json!({ "cash_floor": { "amount": amount, "currency": currency, "period": period } });
    for (body, says) in [
        (
            json!({ "roles": ["a", "b", "c", "d", "e", "f"] }),
            "at most 5",
        ),
        (json!({ "locations": eleven }), "at most 10"),
        (json!({ "avoid": eleven }), "at most 10"),
        (json!({ "must_have": eleven }), "at most 10"),
        (json!({ "roles": [long] }), "longer than 100"),
        (json!({ "seniority": ["wizard"] }), "unknown variant"),
        (json!({ "work_mode": ["underwater"] }), "unknown variant"),
        (json!({ "relocate": "sometimes" }), "unknown variant"),
        (json!({ "company_stage": ["huge"] }), "unknown variant"),
        (json!({ "autonomy": "everything" }), "unknown variant"),
        (floor(0, "EUR", "year"), "between 1 and"),
        (floor(100_000_001, "EUR", "year"), "between 1 and"),
        (floor(5, "EURO", "year"), "ISO 4217"),
        (floor(5, "XXX", "year"), "ISO 4217"),
        (floor(5, "EUR", "week"), "unknown variant"),
        (
            json!({ "languages_for_work": ["english please"] }),
            "language tag",
        ),
        (json!({ "salary": 5 }), "unknown field"),
        (json!("not goals"), ""),
    ] {
        let (status, problem) = h.json("PUT", "/api/v1/goals", &body).await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(problem["code"], "invalid_goals", "{problem}");
        assert!(
            problem["detail"].as_str().unwrap().contains(says),
            "{problem}"
        );
    }
    let (_, view) = h.get("/api/v1/goals").await;
    assert_eq!(view["version"], 0, "nothing was saved");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_limits_themselves_are_accepted(db: PgPool) {
    let h = Harness::new(db, None);
    let ten: Vec<String> = (0..10).map(|n| format!("item {n}")).collect();
    let (status, saved) = h
        .json(
            "PUT",
            "/api/v1/goals",
            &json!({
                "roles": ["a", "b", "c", "d", "e"],
                "locations": ten,
                "avoid": ten,
                "must_have": ten,
                "cash_floor": { "amount": 100_000_000u64, "currency": "JPY", "period": "month" },
            }),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["version"], 1);
}
