//! Reading and editing the profile by hand.

use serde_json::{Value, json};
use sqlx::PgPool;

use crate::testkit::Harness;

#[sqlx::test(migrations = "../../../migrations")]
async fn an_empty_profile_then_one_written_by_hand(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    let (status, view) = h.get("/api/v1/profile").await;
    assert_eq!(status, 200);
    assert_eq!(view["source"], Value::Null);
    assert_eq!(view["updated_at"], Value::Null);
    assert_eq!(view["profile"]["basics"]["profiles"], json!([]));

    let written = json!({
        "basics": { "name": "  Alex Demo ", "label": "Engineer", "summary": "" },
        "work": [{ "name": "Example Org", "position": "Engineer", "startDate": "2020-01",
                   "highlights": ["Shipped the ledger", " "] }],
        "skills": [{ "name": "Rust" }],
    });
    let (status, saved) = h.json("PUT", "/api/v1/profile", &written).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["source"], "manual");
    let profile = &saved["profile"];
    assert_eq!(profile["basics"]["name"], "Alex Demo", "trimmed");
    assert!(
        profile["basics"].get("summary").is_none(),
        "empty is removed"
    );
    assert_eq!(
        profile["work"][0]["highlights"],
        json!(["Shipped the ledger"])
    );

    let (_, again) = h.get("/api/v1/profile").await;
    assert_eq!(again["profile"], *profile);
    assert!(again["updated_at"].is_string());

    // Nobody else sees it.
    let (_, theirs) = Harness::new(db, None).get("/api/v1/profile").await;
    assert_eq!(theirs["profile"]["work"], json!([]));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_profile_that_does_not_fit_is_refused(db: PgPool) {
    let h = Harness::new(db, None);
    for (body, says) in [
        (
            json!({ "work": [{ "name": "Example Org", "startDate": "last spring" }] }),
            "date",
        ),
        (json!({ "skills": [{ "name": "x".repeat(301) }] }), "longer"),
        (
            json!({ "basics": { "birthday": "1990-01-01" } }),
            "birthday",
        ),
        (json!({ "skills": [{ "level": "expert" }] }), "name"),
        (json!("not a profile"), ""),
    ] {
        let (status, problem) = h.json("PUT", "/api/v1/profile", &body).await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(problem["code"], "invalid_profile", "{problem}");
        assert!(
            problem["detail"].as_str().unwrap().contains(says),
            "{problem}"
        );
    }
    assert_eq!(h.count("SELECT count(*) FROM profile.profiles").await, 0);
}
