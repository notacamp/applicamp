//! Grill me over HTTP, against a fake model that answers in turn. All data is
//! invented.

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::MockServer;

use super::{DAILY_CAP, judge::Coverage};
use crate::{
    schema::Area,
    testkit::{Harness, gateway, scripted_model},
};

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

fn reply(value: &Value) -> (&'static str, String) {
    ("stop", value.to_string())
}

fn question(text: &str, area: &str, push: bool) -> Value {
    json!({
        "facts": [], "coverage": [],
        "next": { "question": text, "area": area, "push": push },
        "conflict": null,
    })
}

const ANSWER: &str = "I proposed it and led it. 2 others helped part time. It took 7 months.";

fn read_answer() -> Value {
    json!({
        "facts": [
            { "text": "Proposed and led the rebuild", "quote": "I proposed it and led it",
              "area": "your_part" },
            { "text": "2 others helped part time", "quote": "2 others helped part time",
              "area": "your_part" },
            { "text": "Took 7 months", "quote": "It took 7 months", "area": "results" },
            // Never said.
            { "text": "Saved 30% of cost", "quote": "we saved 30%", "area": "results" },
        ],
        "coverage": [
            { "area": "your_part", "level": "ok" },
            { "area": "results", "level": "part" },
            { "area": "stories", "level": "none" },
        ],
        "next": { "question": "How do you know it took 7 months?", "area": "results", "push": false },
        "conflict": null,
    })
}

/// Facts of `view` still waiting for Keep, Fix or Drop.
fn ready(view: &Value) -> usize {
    view["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["status"] == "ready")
        .count()
}

async fn calls(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

async fn saved(h: &Harness, body: &Value) {
    let (status, saved) = h.json("PUT", "/api/v1/profile", body).await;
    assert_eq!(status, 200, "{saved}");
}

async fn answer(h: &Harness, text: &str) -> (axum::http::StatusCode, Value) {
    h.json(
        "POST",
        "/api/v1/profile/grill/0/answer",
        &json!({ "text": text }),
    )
    .await
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_interview_pushes_back_takes_facts_and_keeps_only_what_the_person_keeps(db: PgPool) {
    let server = scripted_model(vec![
        reply(&question(
            "What was your part in the billing rebuild?",
            "your_part",
            false,
        )),
        reply(&question(
            "\"It went well\" will not hold up. Who decided, how many people, how long?",
            "your_part",
            true,
        )),
        reply(&read_answer()),
    ])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;

    // Nothing is started: the list says so, and no call was made.
    let (status, list) = h.get("/api/v1/profile/grill").await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["roles"].as_array().unwrap().len(), 2);
    assert_eq!(list["roles"][0]["status"], "not_started");
    assert_eq!(list["daily_cap"], DAILY_CAP);
    assert_eq!(calls(&server).await, 0);

    // Start: one call; starting again shows the same question and calls nothing.
    let (status, started) = h.post("/api/v1/profile/grill/0/start?locale=pt-PT").await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["status"], "in_progress");
    assert_eq!(started["question"]["area"], "your_part");
    assert_eq!(started["question"]["push"], false);
    assert_eq!(started["covered"], 0);
    assert_eq!(started["coverage"].as_array().unwrap().len(), 7);
    let (_, again) = h.post("/api/v1/profile/grill/0/start").await;
    assert_eq!(again["turns"].as_array().unwrap().len(), 1);
    assert_eq!(calls(&server).await, 1);
    let seen = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&seen[0].body);
    assert!(body.contains("<language>pt-PT</language>"));
    assert!(body.contains("Example Org") && !body.contains("Alex Demo"));
    assert!(!body.contains("Sample Labs"), "only the one position");

    // A vague answer: pushed back on, no facts.
    let (status, pushed) = answer(&h, "I did the rebuild, it went well.").await;
    assert_eq!(status, 200, "{pushed}");
    assert_eq!(pushed["question"]["push"], true);
    assert_eq!(pushed["proposals"], json!([]));
    assert_eq!(pushed["covered"], 0);

    // A real answer: three facts, one dropped, coverage up.
    let (status, read) = answer(&h, ANSWER).await;
    assert_eq!(status, 200, "{read}");
    let proposals = read["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 3);
    assert_eq!(proposals[0]["quote"], "I proposed it and led it");
    assert!(
        !read.to_string().contains("30%"),
        "an unsaid fact never shows"
    );
    let answer_turn = read["turns"]
        .as_array()
        .unwrap()
        .iter()
        .rfind(|t| t["kind"] == "answer")
        .unwrap();
    assert_eq!(answer_turn["dropped"], 1);
    assert_eq!(read["covered"], 1);
    assert_eq!(
        read["question"]["text"],
        "How do you know it took 7 months?"
    );
    assert_eq!(calls(&server).await, 3);
    // Nothing is in the profile yet.
    let (_, view) = h.get("/api/v1/profile").await;
    assert!(view["profile"]["work"][0].get("facts").is_none());

    // Keep one as it is.
    let first = proposals[0]["id"].as_str().unwrap();
    let (status, kept) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{first}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(status, 200, "{kept}");
    assert_eq!(kept["kept"], 1);
    assert_eq!(ready(&kept), 2);
    assert_eq!(kept["proposals"][0]["status"], "kept");

    // Fix one: a claim neither the quote nor the profile makes is refused.
    let second = proposals[1]["id"].as_str().unwrap();
    let (status, refused) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{second}/keep"),
            &json!({ "text": "9 others helped full time" }),
        )
        .await;
    assert_eq!(status, 400, "{refused}");
    assert_eq!(refused["code"], "fact_not_supported");
    let (status, fixed) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{second}/keep"),
            &json!({ "text": "Two others helped part time" }),
        )
        .await;
    assert_eq!(status, 200, "{fixed}");
    assert_eq!(fixed["kept"], 2);

    // Drop the last.
    let third = proposals[2]["id"].as_str().unwrap();
    let (status, dropped) = h
        .post(&format!("/api/v1/profile/grill/proposals/{third}/drop"))
        .await;
    assert_eq!(status, 200, "{dropped}");
    assert_eq!(ready(&dropped), 0);
    assert_eq!(dropped["proposals"][2]["status"], "dropped");
    assert_eq!(
        dropped["proposals"][1]["text"],
        "Two others helped part time"
    );
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
    assert_eq!(facts[1]["area"], "your_part");
    assert!(view["profile"]["work"][1].get("facts").is_none());

    // Kept twice is a conflict; dropped twice is fine.
    let (status, twice) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{first}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(status, 409, "{twice}");
    assert_eq!(twice["code"], "proposal_not_ready");
    let (status, _) = h
        .post(&format!("/api/v1/profile/grill/proposals/{third}/drop"))
        .await;
    assert_eq!(status, 200);

    // The list shows the progress.
    let (_, list) = h.get("/api/v1/profile/grill").await;
    assert_eq!(list["roles"][0]["status"], "in_progress");
    assert_eq!(list["roles"][0]["covered"], 1);
    assert_eq!(list["roles"][0]["kept"], 2);
    assert_eq!(list["roles"][1]["status"], "not_started");
    assert_eq!(
        calls(&server).await,
        3,
        "keeping and dropping never call the model"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn keep_all_writes_every_waiting_fact_in_one_profile_version(db: PgPool) {
    let server = scripted_model(vec![
        reply(&question("What was your part?", "your_part", false)),
        reply(&read_answer()),
    ])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/grill/0/start").await;
    answer(&h, ANSWER).await;
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
async fn skipping_leaves_the_area_alone_and_done_calls_nothing(db: PgPool) {
    let server = scripted_model(vec![
        reply(&question("What did the company sell?", "context", false)),
        reply(&question("What did you do yourself?", "your_part", false)),
    ])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/grill/0/start").await;

    let (status, skipped) = h.post("/api/v1/profile/grill/0/skip").await;
    assert_eq!(status, 200, "{skipped}");
    assert_eq!(skipped["question"]["area"], "your_part");
    let kinds: Vec<&str> = skipped["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["question", "note", "question"]);
    assert_eq!(skipped["turns"][1]["text"], "skipped");
    assert_eq!(calls(&server).await, 2);
    let skipped_areas: Value =
        sqlx::query_scalar("SELECT skipped FROM profile.grill_sessions WHERE account_id = $1")
            .bind(h.account)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(skipped_areas, json!(["context"]));

    // Done: no call, the question is put away, and answering is refused.
    let (status, done) = h.post("/api/v1/profile/grill/0/done").await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["question"], Value::Null);
    assert_eq!(calls(&server).await, 2);
    let (status, none) = answer(&h, "More.").await;
    assert_eq!(status, 409, "{none}");
    assert_eq!(none["code"], "no_open_question");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_account_stops_at_the_daily_cap_without_calling_the_model(db: PgPool) {
    let server = scripted_model(vec![reply(&question(
        "What was your part?",
        "your_part",
        false,
    ))])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/grill/0/start").await;
    assert_eq!(calls(&server).await, 1);

    let session: Uuid =
        sqlx::query_scalar("SELECT id FROM profile.grill_sessions WHERE account_id = $1")
            .bind(h.account)
            .fetch_one(&h.db)
            .await
            .unwrap();
    // 149 more calls today, then a question waiting.
    for seq in 100..249 {
        sqlx::query(
            "INSERT INTO profile.grill_turns (session_id, seq, account_id, kind, text, ai_call, created_at)
             VALUES ($1, $2, $3, 'note', 'skipped', true, now())",
        )
        .bind(session)
        .bind(seq)
        .bind(h.account)
        .execute(&h.db)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO profile.grill_turns (session_id, seq, account_id, kind, text, created_at)
         VALUES ($1, 300, $2, 'question', 'Anything else?', now())",
    )
    .bind(session)
    .bind(h.account)
    .execute(&h.db)
    .await
    .unwrap();

    let (status, capped) = answer(&h, "A fair answer with 3 numbers.").await;
    assert_eq!(status, 429, "{capped}");
    assert_eq!(capped["code"], "grill_daily_cap");
    let (status, capped) = h.post("/api/v1/profile/grill/0/skip").await;
    assert_eq!(status, 429, "{capped}");
    assert_eq!(calls(&server).await, 1, "no call was made over the cap");

    // The view says where the account is.
    let (_, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(view["calls_today"], 150);
    assert_eq!(view["daily_cap"], 150);

    // Yesterday's calls do not count.
    sqlx::query(
        "UPDATE profile.grill_turns SET created_at = now() - interval '2 days' WHERE ai_call",
    )
    .execute(&h.db)
    .await
    .unwrap();
    let (_, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(view["calls_today"], 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn bad_input_unknown_positions_and_other_accounts_are_refused(db: PgPool) {
    let server = scripted_model(vec![reply(&question(
        "What was your part?",
        "your_part",
        false,
    ))])
    .await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    saved(&h, &profile()).await;

    let (status, _) = h.get("/api/v1/profile/grill/9").await;
    assert_eq!(status, 404);
    let (status, _) = h.post("/api/v1/profile/grill/9/start").await;
    assert_eq!(status, 404);
    // Not started: an answer has nothing to answer.
    let (status, _) = answer(&h, "Hello").await;
    assert_eq!(status, 404);

    h.post("/api/v1/profile/grill/0/start").await;
    let long = "x".repeat(2_001);
    for text in ["", "   ", long.as_str()] {
        let (status, bad) = answer(&h, text).await;
        assert_eq!(status, 400, "{bad}");
        assert_eq!(bad["code"], "invalid_answer");
    }
    let (status, bad) = h
        .json(
            "POST",
            "/api/v1/profile/grill/0/answer",
            &json!({ "nope": 1 }),
        )
        .await;
    assert_eq!(status, 400, "{bad}");
    assert_eq!(calls(&server).await, 1);

    // Another account sees nothing of it.
    let other = Harness::as_account(db, Some(gateway(&server, None)), Uuid::now_v7());
    let (status, _) = other.get("/api/v1/profile/grill/0").await;
    assert_eq!(status, 404);
    let (status, list) = other.get("/api/v1/profile/grill").await;
    assert_eq!(status, 200);
    assert_eq!(list["roles"], json!([]));
    let (status, _) = other
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{}/keep", Uuid::now_v7()),
            &json!({}),
        )
        .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_a_model_it_says_so(db: PgPool) {
    let h = Harness::new(db, None);
    saved(&h, &profile()).await;
    let (status, none) = h.post("/api/v1/profile/grill/0/start").await;
    assert_eq!(status, 503, "{none}");
    assert_eq!(none["code"], "ai_unavailable");
    // Reading the list needs no model.
    let (status, _) = h.get("/api/v1/profile/grill").await;
    assert_eq!(status, 200);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_position_that_moved_does_not_borrow_the_interview_of_the_one_before(db: PgPool) {
    let server = scripted_model(vec![reply(&question(
        "What was your part?",
        "your_part",
        false,
    ))])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/grill/0/start").await;

    // The positions swap places.
    let mut swapped = profile();
    swapped["work"].as_array_mut().unwrap().reverse();
    saved(&h, &swapped).await;
    let (status, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(status, 200);
    assert_eq!(view["status"], "not_started");
    assert_eq!(view["turns"], json!([]));
    assert_eq!(view["role"]["company"], "Sample Labs");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn facts_the_person_waited_too_long_to_keep_are_gone_and_cannot_be_kept(db: PgPool) {
    let server = scripted_model(vec![
        reply(&question("What was your part?", "your_part", false)),
        reply(&read_answer()),
    ])
    .await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/grill/0/start").await;
    let (_, read) = answer(&h, ANSWER).await;
    let id = read["proposals"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        h.count("SELECT count(*) FROM profile.grill_proposals")
            .await,
        3
    );

    sqlx::query("UPDATE profile.grill_proposals SET expires_at = now() - interval '1 minute'")
        .execute(&h.db)
        .await
        .unwrap();
    let (_, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(view["proposals"], json!([]));
    let (status, _) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{id}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(status, 404);
}

#[test]
fn coverage_is_stored_as_a_plain_object_of_areas() {
    let mut coverage = Coverage::default();
    coverage.raise(Area::YourPart, super::judge::CoverageLevel::Ok);
    coverage.raise(Area::Results, super::judge::CoverageLevel::Part);
    let json = serde_json::to_value(&coverage).unwrap();
    assert_eq!(json, json!({ "your_part": "ok", "results": "part" }));
    let back: Coverage = serde_json::from_value(json).unwrap();
    assert_eq!(back, coverage);
}

#[test]
fn the_daily_cap_is_150_calls() {
    assert!(!super::over_cap(DAILY_CAP - 1));
    assert!(super::over_cap(DAILY_CAP));
    assert_eq!(DAILY_CAP, 150);
}
