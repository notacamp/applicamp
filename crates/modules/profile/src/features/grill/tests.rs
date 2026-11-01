//! The facts taken from an answer about a job, kept, fixed or dropped over
//! HTTP. The facts are put where `about::propose` puts them, so no model is
//! involved. All data is invented.

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::testkit::Harness;

fn profile() -> Value {
    json!({
        "basics": { "name": "Alex Demo" },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust on PostgreSQL." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02", "summary": "Built the data pipeline." },
        ],
    })
}

/// The three facts an answer came to: text, quote, area.
const FACTS: [(&str, &str, &str); 3] = [
    (
        "Proposed and led the rebuild",
        "I proposed it and led it",
        "your_part",
    ),
    (
        "2 others helped part time",
        "2 others helped part time",
        "your_part",
    ),
    ("Took 7 months", "It took 7 months", "results"),
];

/// Facts of `view` still waiting for Keep, Fix or Drop.
fn ready(view: &Value) -> usize {
    view["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["status"] == "ready")
        .count()
}

async fn saved(h: &Harness, body: &Value) {
    let (status, saved) = h.json("PUT", "/api/v1/profile", body).await;
    assert_eq!(status, 200, "{saved}");
}

/// Wait the three facts on position 0, as an answer about a job leaves them.
async fn propose(h: &Harness) -> Vec<Uuid> {
    let role_key = super::role_key(&serde_json::from_value(profile()["work"][0].clone()).unwrap());
    let session = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO profile.grill_sessions
            (id, account_id, position, role_key, created_at, updated_at)
         VALUES ($1, $2, 0, $3, now(), now())",
    )
    .bind(session)
    .bind(h.account)
    .bind(role_key)
    .execute(&h.db)
    .await
    .unwrap();
    let mut ids = Vec::new();
    for (text, quote, area) in FACTS {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO profile.grill_proposals
                (id, session_id, account_id, position, turn_seq, text, quote, area, status,
                 created_at, expires_at)
             VALUES ($1, $2, $3, 0, 0, $4, $5, $6, 'ready', now(), now() + interval '1 day')",
        )
        .bind(id)
        .bind(session)
        .bind(h.account)
        .bind(text)
        .bind(quote)
        .bind(area)
        .execute(&h.db)
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_person_keeps_fixes_and_drops_what_an_answer_came_to(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    let ids = propose(&h).await;

    let (status, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(ready(&view), 3);
    assert_eq!(view["kept"], 0);
    assert_eq!(view["role"]["company"], "Example Org");
    // Nothing is in the profile yet.
    let (_, profile_view) = h.get("/api/v1/profile").await;
    assert!(profile_view["profile"]["work"][0].get("facts").is_none());

    // Keep one as it is.
    let (status, kept) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", ids[0]),
            &json!({}),
        )
        .await;
    assert_eq!(status, 200, "{kept}");
    assert_eq!(kept["kept"], 1);
    assert_eq!(ready(&kept), 2);

    // Fix one: a claim neither the quote nor the profile makes is refused.
    let (status, refused) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", ids[1]),
            &json!({ "text": "9 others helped full time" }),
        )
        .await;
    assert_eq!(status, 400, "{refused}");
    assert_eq!(refused["code"], "fact_not_supported");
    let (status, fixed) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", ids[1]),
            &json!({ "text": "Two others helped part time" }),
        )
        .await;
    assert_eq!(status, 200, "{fixed}");
    assert_eq!(fixed["kept"], 2);

    // Drop the last.
    let (status, dropped) = h
        .post(&format!("/api/v1/profile/grill/proposals/{}/drop", ids[2]))
        .await;
    assert_eq!(status, 200, "{dropped}");
    assert_eq!(ready(&dropped), 0);
    assert_eq!(dropped["kept"], 2);

    // The profile holds the two kept facts, each with the person's own words.
    let (_, view) = h.get("/api/v1/profile").await;
    let facts = view["profile"]["work"][0]["facts"].as_array().unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["quote"], "I proposed it and led it");
    assert_eq!(facts[1]["text"], "Two others helped part time");
    assert_eq!(
        facts[1]["quote"], "2 others helped part time",
        "the quote never changes"
    );
    assert!(view["profile"]["work"][1].get("facts").is_none());

    // Kept twice is a conflict; dropped twice is fine.
    let (status, twice) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", ids[0]),
            &json!({}),
        )
        .await;
    assert_eq!(status, 409, "{twice}");
    assert_eq!(twice["code"], "proposal_not_ready");
    let (status, _) = h
        .post(&format!("/api/v1/profile/grill/proposals/{}/drop", ids[2]))
        .await;
    assert_eq!(status, 200);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn keep_all_writes_every_waiting_fact_in_one_profile_version(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    propose(&h).await;
    let before: i32 =
        sqlx::query_scalar("SELECT version FROM profile.profiles WHERE account_id = $1")
            .bind(h.account)
            .fetch_one(&h.db)
            .await
            .unwrap();

    let (status, view) = h.post("/api/v1/profile/grill/0/keep-all").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(ready(&view), 0);
    assert_eq!(view["kept"], 3);
    let after: i32 =
        sqlx::query_scalar("SELECT version FROM profile.profiles WHERE account_id = $1")
            .bind(h.account)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(after, before + 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn unknown_positions_and_other_accounts_are_refused(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    saved(&h, &profile()).await;
    let ids = propose(&h).await;

    let (status, _) = h.get("/api/v1/profile/grill/9").await;
    assert_eq!(status, 404);
    let (status, _) = h.post("/api/v1/profile/grill/9/keep-all").await;
    assert_eq!(status, 404);

    // Another account sees nothing of it.
    let other = Harness::as_account(db, None, Uuid::now_v7());
    let (status, _) = other.get("/api/v1/profile/grill/0").await;
    assert_eq!(status, 404);
    for action in ["keep", "drop"] {
        let (status, _) = other
            .json(
                "POST",
                &format!("/api/v1/profile/grill/proposals/{}/{action}", ids[0]),
                &json!({}),
            )
            .await;
        assert_eq!(status, 404, "{action}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_position_that_moved_does_not_borrow_the_facts_of_the_one_before(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    propose(&h).await;

    // The positions swap places.
    let mut swapped = profile();
    swapped["work"].as_array_mut().unwrap().reverse();
    saved(&h, &swapped).await;
    let (status, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(status, 200);
    assert_eq!(view["proposals"], json!([]));
    assert_eq!(view["role"]["company"], "Sample Labs");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn facts_the_person_waited_too_long_to_keep_are_gone_and_cannot_be_kept(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    let ids = propose(&h).await;

    sqlx::query("UPDATE profile.grill_proposals SET expires_at = now() - interval '1 minute'")
        .execute(&h.db)
        .await
        .unwrap();
    let (_, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(view["proposals"], json!([]));
    let (status, _) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", ids[0]),
            &json!({}),
        )
        .await;
    assert_eq!(status, 404);
}
