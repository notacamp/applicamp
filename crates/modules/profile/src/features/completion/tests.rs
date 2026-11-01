//! Profile completion over HTTP: the strength and the order of the skills.
//! All data is invented.

use serde_json::{Value, json};
use sqlx::PgPool;

use crate::testkit::Harness;

/// A profile of three positions without results, a skill and no headline.
fn profile() -> Value {
    json!({
        "basics": { "name": "Alex Demo" },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust on PostgreSQL." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02",
              "summary": "Built the data pipeline with Kafka and Rust." },
            { "name": "Third Co", "position": "Developer", "startDate": "2014-01",
              "endDate": "2017-05", "summary": "Wrote internal tools." },
        ],
        "skills": [{ "name": "PostgreSQL" }],
    })
}

async fn saved(h: &Harness, body: &Value) {
    let (status, saved) = h.json("PUT", "/api/v1/profile", body).await;
    assert_eq!(status, 200, "{saved}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn skills_can_be_reordered(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    // The order can change.
    let (status, saved_view) = h
        .json(
            "PUT",
            "/api/v1/profile",
            &json!({ "basics": { "name": "Alex Demo" }, "skills": [{ "name": "PostgreSQL" }, { "name": "Rust" }] }),
        )
        .await;
    assert_eq!(status, 200, "{saved_view}");
    let (status, ordered) = h
        .json(
            "PUT",
            "/api/v1/profile/skills/order",
            &json!({ "names": ["Rust", "PostgreSQL"] }),
        )
        .await;
    assert_eq!(status, 200, "{ordered}");
    assert_eq!(ordered["profile"]["skills"][0]["name"], "Rust");
    for bad in [
        json!(["Rust"]),
        json!(["Rust", "Rust"]),
        json!(["Rust", "Go"]),
    ] {
        let (status, problem) = h
            .json(
                "PUT",
                "/api/v1/profile/skills/order",
                &json!({ "names": bad }),
            )
            .await;
        assert_eq!(
            (status.as_u16(), problem["code"].as_str()),
            (400, Some("invalid_order")),
            "{bad}"
        );
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_strength_names_what_to_do_and_needs_no_model(db: PgPool) {
    let h = Harness::new(db, None);
    let (_, empty) = h.get("/api/v1/profile/strength").await;
    assert_eq!(empty["score"], 0);
    assert_eq!(empty["strong"], false);

    saved(&h, &profile()).await;
    let (_, strength) = h.get("/api/v1/profile/strength").await;
    let items = strength["items"].as_array().unwrap();
    let action = |key: &str| {
        items.iter().find(|i| i["key"] == key).unwrap()["action"]
            .as_str()
            .map(str::to_owned)
    };
    // The checklist needs no review.
    assert_eq!(action("headline").as_deref(), Some("edit:basics"));
    assert_eq!(action("skills").as_deref(), Some("skills"));
    assert_eq!(action("positions"), None, "three dated positions are full");
    assert_eq!(action("goals").as_deref(), Some("goals"));
    assert_eq!(strength["questions_left"], Value::Null);
}
