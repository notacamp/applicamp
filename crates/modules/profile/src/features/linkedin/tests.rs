//! Your LinkedIn over HTTP, against a fake model that answers a review. All
//! data is invented.

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

use crate::{
    features::imports::testkit::export_zip,
    testkit::{Harness, Part, gateway, scripted_model},
};

/// The master profile: the same person as the export, known a little better
/// (Kafka), and never their contact details.
fn profile() -> Value {
    json!({
        "basics": {
            "name": "Alex Demo", "label": "Backend engineer",
            "email": "alex.demo@example.com", "location": "Lisbon, Portugal",
            "profiles": [],
        },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust, with Kafka between its services.",
              "highlights": [] },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02", "summary": "Built the data pipeline.", "highlights": [] },
        ],
        "skills": [{ "name": "Rust" }, { "name": "Kafka" }],
    })
}

fn proposal(field: &str, text: &str, sources: &[&str]) -> Value {
    json!({ "field": field, "text": text, "sources": sources, "why": "Says what you want next." })
}

fn finding(part: &str, position: Option<u32>, state: &str, proposals: Vec<Value>) -> Value {
    json!({ "part": part, "position": position, "state": state,
            "why": "A recruiter for a Staff role looks for this.", "proposals": proposals })
}

/// The model's review: a clean headline, an About text with a buzzword, a
/// description with a tool no source names, and skills (one already on
/// LinkedIn, one new).
fn review_reply() -> (&'static str, String) {
    (
        "stop",
        json!({
            "overall": "Your headline says what you do, not the Staff role you want.",
            "findings": [
                finding("headline", None, "could_be_better", vec![proposal(
                    "headline", "Senior Engineer · billing platform in Rust and Kafka",
                    &["w0", "l.headline"],
                )]),
                finding("summary", None, "missing", vec![proposal(
                    "summary", "Passionate backend engineer who runs the billing platform in Rust.",
                    &["w0"],
                )]),
                finding("position", Some(0), "could_be_better", vec![proposal(
                    "position_description", "Runs the billing platform in Rust on Kubernetes.",
                    &["w0"],
                )]),
                finding("skills", None, "could_be_better", vec![
                    proposal("skill", "Rust", &["w0"]),
                    proposal("skill", "Kafka", &["w0"]),
                    proposal("skill", "PostgreSQL", &["l.skills"]),
                ]),
                finding("education", None, "good", vec![]),
            ],
        })
        .to_string(),
    )
}

async fn seeded(db: PgPool, server: &MockServer) -> Harness {
    let h = Harness::new(db, Some(gateway(server, None)));
    let (status, saved) = h.json("PUT", "/api/v1/profile", &profile()).await;
    assert_eq!(status, 200, "{saved}");
    let (status, goals) = h
        .json(
            "PUT",
            "/api/v1/goals",
            &json!({ "roles": ["Staff Engineer"] }),
        )
        .await;
    assert_eq!(status, 200, "{goals}");
    h
}

async fn check(h: &Harness, zip: &[u8]) -> (axum::http::StatusCode, Value) {
    h.upload_to(
        "/api/v1/profile/linkedin?locale=en",
        &[
            Part::Text("kind", "linkedin_zip"),
            Part::File("file", "Basic_LinkedInDataExport.zip", zip),
        ],
    )
    .await
}

fn proposals(view: &Value) -> &Vec<Value> {
    view["review"]["proposals"].as_array().unwrap()
}

fn find<'a>(view: &'a Value, id: &str) -> &'a Value {
    proposals(view)
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("no proposal {id} in {view}"))
}

async fn decide(h: &Harness, id: &str, body: &Value) -> (axum::http::StatusCode, Value) {
    h.json(
        "PATCH",
        &format!("/api/v1/profile/linkedin/proposals/{id}"),
        body,
    )
    .await
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_checkup_from_the_export_reviews_linkedin_against_the_profile_and_goals(db: PgPool) {
    let server = scripted_model(vec![review_reply()]).await;
    let h = seeded(db, &server).await;

    let (status, before) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(before["has_profile"], true);
    assert_eq!(before["ai_available"], true);
    assert_eq!(before["review"], Value::Null);
    assert_eq!(before["daily_cap"], 10);

    let (status, view) = check(&h, &export_zip()).await;
    assert_eq!(status, 201, "{view}");
    let review = &view["review"];
    assert_eq!(review["kind"], "linkedin_zip");
    assert_eq!(review["language"], "en");
    assert_eq!(review["stale"], false);
    assert!(review["overall"].as_str().unwrap().contains("Staff role"));

    // One finding per part, with what LinkedIn says there now.
    let findings = review["findings"].as_array().unwrap();
    let parts: Vec<&str> = findings
        .iter()
        .map(|f| f["part"].as_str().unwrap())
        .collect();
    assert_eq!(
        parts,
        ["headline", "summary", "position", "skills", "education"]
    );
    assert_eq!(findings[0]["state"], "could_be_better");
    assert_eq!(findings[0]["now"], "Backend engineer · Rust and Postgres");
    assert_eq!(findings[1]["state"], "missing");
    assert_eq!(findings[2]["position"], 0);
    assert_eq!(findings[2]["title"], "Senior Engineer at Example Org");
    assert_eq!(findings[2]["now"], "Runs the billing platform.");
    assert_eq!(findings[3]["now"], "Rust, PostgreSQL, TypeScript");
    assert_eq!(findings[4]["state"], "good");
    assert_eq!(findings[4]["proposals"], json!([]));

    // The proposals: a skill already on LinkedIn is not proposed again.
    let texts: Vec<&str> = proposals(&view)
        .iter()
        .map(|p| p["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        texts,
        [
            "Senior Engineer · billing platform in Rust and Kafka",
            "Passionate backend engineer who runs the billing platform in Rust.",
            "Runs the billing platform in Rust on Kubernetes.",
            "Kafka",
        ]
    );
    let headline = find(&view, "p1");
    assert_eq!(headline["field"], "headline");
    assert_eq!(headline["flags"], json!([]));
    assert_eq!(headline["status"], "ready");
    assert_eq!(headline["sources"][0]["kind"], "profile");
    assert_eq!(headline["sources"][1]["id"], "l.headline");
    assert_eq!(headline["sources"][1]["kind"], "linkedin");
    // A buzzword no source says, and a tool no source names: flagged, kept.
    assert_eq!(
        find(&view, "p2")["flags"],
        json!([{ "kind": "buzzword", "claim": "passionate" }])
    );
    assert_eq!(
        find(&view, "p3")["flags"],
        json!([{ "kind": "claim", "claim": "Kubernetes" }])
    );
    assert_eq!(find(&view, "p3")["position"], 0);
    assert_eq!(find(&view, "p4")["field"], "skill");
    assert_eq!(review["open"], 4);
    assert_eq!(review["kept"], 0);
    assert_eq!(view["checkups_today"], 1);

    // One model call, shown the goals and LinkedIn's own words, never who
    // the person is.
    let sent = server.received_requests().await.unwrap();
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8(sent[0].body.clone()).unwrap();
    for shown in [
        "Staff Engineer",
        "Backend engineer · Rust and Postgres",
        "Kafka",
        "\"l.skills\"",
    ] {
        assert!(body.contains(shown), "{shown} not in the prompt");
    }
    for private in ["Alex", "Demo", "example.com", "Lisbon", "PORTFOLIO"] {
        assert!(!body.contains(private), "{private} in the prompt");
    }
    // Nor does the stored LinkedIn profile say who they are.
    let stored: String = sqlx::query_scalar("SELECT linkedin::text FROM profile.linkedin_reviews")
        .fetch_one(&h.db)
        .await
        .unwrap();
    for private in ["Alex", "alex.demo", "Lisbon", "demo.example.com"] {
        assert!(!stored.contains(private), "{private} stored");
    }

    // Reading it back costs nothing and says the same.
    let (_, again) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(again["review"]["proposals"], view["review"]["proposals"]);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    // The profile is untouched.
    let (_, profile) = h.get("/api/v1/profile").await;
    assert_eq!(profile["profile"]["basics"]["label"], "Backend engineer");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn keep_edit_drop_and_done_each_hold_their_rules_and_can_be_taken_back(db: PgPool) {
    let server = scripted_model(vec![review_reply()]).await;
    let h = seeded(db, &server).await;
    let (status, view) = check(&h, &export_zip()).await;
    assert_eq!(status, 201, "{view}");

    // A flagged proposal cannot be kept as it is.
    let (status, problem) = decide(&h, "p3", &json!({ "status": "kept" })).await;
    assert_eq!(status, 409, "{problem}");
    assert_eq!(problem["code"], "proposal_flagged");
    // Edited to say only what the sources say, it passes, and is ready again.
    let (status, view) = decide(
        &h,
        "p3",
        &json!({ "text": "Runs the billing platform in Rust." }),
    )
    .await;
    assert_eq!(status, 200, "{view}");
    let edited = find(&view, "p3");
    assert_eq!(edited["flags"], json!([]));
    assert_eq!(edited["edited"], true);
    assert_eq!(edited["status"], "ready");
    // An edit that still claims too much is flagged again, never refused.
    let (status, view) = decide(&h, "p3", &json!({ "text": "Runs billing on Kubernetes." })).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(find(&view, "p3")["flags"][0]["claim"], "Kubernetes");
    let (_, view) = decide(
        &h,
        "p3",
        &json!({ "text": "Runs the billing platform in Rust." }),
    )
    .await;
    assert_eq!(find(&view, "p3")["flags"], json!([]));

    // Keep, then done; done needs kept first.
    let (status, problem) = decide(&h, "p1", &json!({ "status": "done" })).await;
    assert_eq!(status, 409, "{problem}");
    assert_eq!(problem["code"], "proposal_not_kept");
    let (_, view) = decide(&h, "p1", &json!({ "status": "kept" })).await;
    assert_eq!(find(&view, "p1")["status"], "kept");
    assert_eq!(view["review"]["kept"], 1);
    let (_, view) = decide(&h, "p1", &json!({ "status": "done" })).await;
    assert_eq!(find(&view, "p1")["status"], "done");
    assert_eq!(view["review"]["done"], 1);
    assert_eq!(view["review"]["kept"], 0);
    // Taken back.
    let (_, view) = decide(&h, "p1", &json!({ "status": "kept" })).await;
    assert_eq!(find(&view, "p1")["status"], "kept");
    let (_, view) = decide(&h, "p1", &json!({ "status": "ready" })).await;
    assert_eq!(find(&view, "p1")["status"], "ready");
    // Dropped, and back.
    let (_, view) = decide(&h, "p2", &json!({ "status": "dropped" })).await;
    assert_eq!(find(&view, "p2")["status"], "dropped");
    assert_eq!(view["review"]["open"], 3);
    let (_, view) = decide(&h, "p2", &json!({ "status": "ready" })).await;
    assert_eq!(view["review"]["open"], 4);
    // A rewrite of a kept proposal makes it ready again, unless kept with it.
    let (_, view) = decide(&h, "p4", &json!({ "status": "kept" })).await;
    assert_eq!(find(&view, "p4")["status"], "kept");
    let (_, view) = decide(&h, "p4", &json!({ "text": "Apache Kafka" })).await;
    assert_eq!(find(&view, "p4")["status"], "ready");
    let (_, view) = decide(&h, "p4", &json!({ "text": "Kafka", "status": "kept" })).await;
    assert_eq!(find(&view, "p4")["status"], "kept");

    // What is refused.
    let (status, problem) = decide(&h, "p1", &json!({})).await;
    assert_eq!(status, 400, "{problem}");
    assert_eq!(problem["code"], "invalid_decision");
    let (status, problem) = decide(&h, "p1", &json!({ "text": "   " })).await;
    assert_eq!(status, 400, "{problem}");
    assert_eq!(problem["code"], "invalid_text");
    let (status, problem) = decide(&h, "p1", &json!({ "text": "x".repeat(221) })).await;
    assert_eq!(status, 400, "{problem}");
    let (status, _) = decide(&h, "p9", &json!({ "status": "kept" })).await;
    assert_eq!(status, 404);
    // Decisions never call the model.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_changed_profile_makes_the_review_stale_and_another_account_sees_nothing(db: PgPool) {
    let server = scripted_model(vec![review_reply()]).await;
    let h = seeded(db.clone(), &server).await;
    let (status, _) = check(&h, &export_zip()).await;
    assert_eq!(status, 201);

    let mut changed = profile();
    changed["basics"]["label"] = json!("Staff Engineer");
    let (status, _) = h.json("PUT", "/api/v1/profile", &changed).await;
    assert_eq!(status, 200);
    let (_, view) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(view["review"]["stale"], true);
    assert_eq!(proposals(&view).len(), 4);
    let (status, view) = decide(&h, "p1", &json!({ "status": "kept" })).await;
    assert_eq!(status, 200, "{view}");

    let other = Harness::as_account(db, Some(gateway(&server, None)), uuid::Uuid::now_v7());
    let (_, view) = other.get("/api/v1/profile/linkedin").await;
    assert_eq!(view["review"], Value::Null);
    assert_eq!(view["has_profile"], false);
    let (status, _) = decide(&other, "p1", &json!({ "status": "kept" })).await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn no_profile_no_model_a_wrong_file_and_the_daily_cap_each_refuse(db: PgPool) {
    let server = scripted_model(vec![review_reply()]).await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    let (status, problem) = check(&h, &export_zip()).await;
    assert_eq!(status, 409, "{problem}");
    assert_eq!(problem["code"], "profile_needed");

    let h = seeded(db.clone(), &server).await;
    // A file that is no export costs nothing and counts for nothing.
    let (status, problem) = check(&h, b"not a zip").await;
    assert_eq!(status, 400, "{problem}");
    assert_eq!(problem["code"], "not_a_zip");
    let (status, problem) = h
        .upload_to(
            "/api/v1/profile/linkedin",
            &[Part::Text("kind", "text"), Part::Text("text", "my profile")],
        )
        .await;
    assert_eq!(status, 400, "{problem}");
    assert_eq!(problem["code"], "invalid_kind");
    let (_, view) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(view["checkups_today"], 0);
    assert_eq!(server.received_requests().await.unwrap().len(), 0);

    // The day's cap.
    for _ in 0..10 {
        sqlx::query(
            "INSERT INTO profile.linkedin_calls (id, account_id, created_at) VALUES ($1, $2, now())",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(h.account)
        .execute(&h.db)
        .await
        .unwrap();
    }
    let (status, problem) = check(&h, &export_zip()).await;
    assert_eq!(status, 429, "{problem}");
    assert_eq!(problem["code"], "linkedin_daily_cap");

    // No model at all: the page says so, and nothing can be made.
    let h = Harness::as_account(db, None, h.account);
    let (_, view) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(view["ai_available"], false);
    let (status, problem) = check(&h, &export_zip()).await;
    assert_eq!(status, 503, "{problem}");
    assert_eq!(problem["code"], "ai_unavailable");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn start_over_throws_the_review_away_and_an_untouched_one_expires(db: PgPool) {
    let server = scripted_model(vec![review_reply()]).await;
    let h = seeded(db, &server).await;
    let (status, _) = check(&h, &export_zip()).await;
    assert_eq!(status, 201);
    let (status, _) = h
        .send(
            axum::http::Request::delete("/api/v1/profile/linkedin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, 204);
    let (_, view) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(view["review"], Value::Null);
    assert_eq!(view["checkups_today"], 1, "the check-up still counts");

    // A new check-up replaces nothing now; left alone, it expires.
    let (status, _) = check(&h, &export_zip()).await;
    assert_eq!(status, 201);
    sqlx::query("UPDATE profile.linkedin_reviews SET expires_at = now() - interval '1 minute'")
        .execute(&h.db)
        .await
        .unwrap();
    let (_, view) = h.get("/api/v1/profile/linkedin").await;
    assert_eq!(
        view["review"],
        Value::Null,
        "an expired review is not shown"
    );
    // The sweep was queued for the day it expires; it is due now.
    sqlx::query("UPDATE kernel.jobs SET run_at = now() WHERE kind = 'profile.expire_imports'")
        .execute(&h.db)
        .await
        .unwrap();
    h.work().await;
    assert_eq!(
        h.count("SELECT count(*) FROM profile.linkedin_reviews")
            .await,
        0
    );
}
