use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    ports::Posting,
    store::{self, Found},
    testkit::{Fake, Harness},
};

fn found(source_ref: &str, title: &str) -> Found {
    Found {
        source: "greenhouse".into(),
        source_ref: source_ref.into(),
        title: title.into(),
        company: "Example Co".into(),
        location: Some("Remote - Europe".into()),
        work_mode: Some("remote".into()),
        url: format!("https://example.com/jobs/{source_ref}"),
        description: Some("Build things.".into()),
        match_score: 82,
        why: vec!["Six years of Rust on your profile".into()],
        gaps: vec!["Kafka is not on your profile".into()],
        ..Found::default()
    }
}

async fn seed(h: &Harness) {
    let added = store::add_found(
        &h.db,
        &Default::default(),
        h.account,
        vec![
            found("exampleco:1", "Staff Engineer"),
            found("exampleco:2", "Platform Engineer"),
        ],
        Utc::now(),
    )
    .await
    .unwrap();
    assert_eq!(added, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn moving_a_job_forward_keeps_the_change(db: PgPool) {
    let h = Harness::new(db);
    seed(&h).await;

    let (status, body) = h.get("/api/v1/opportunities").await;
    assert_eq!(status, StatusCode::OK);
    let list = body["opportunities"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["stage"], "found");
    assert_eq!(list[0]["new"], true);
    assert_eq!(list[0]["match"], 82);
    assert!(list[0].get("description").is_none(), "the list is light");
    let id = list[0]["id"].as_str().unwrap().to_owned();

    let (status, body) = h.get(&format!("/api/v1/opportunities/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["why"][0], "Six years of Rust on your profile");
    assert_eq!(body["gaps"][0], "Kafka is not on your profile");
    assert_eq!(body["editable"], false);

    // Opening it in the peek: no longer new.
    let (status, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "opened": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["new"], false);

    let (status, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "stage": "shortlisted", "next_step": "Tailor the CV", "notes": "Ask about the team" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "shortlisted");
    assert_eq!(body["next_step"], "Tailor the CV");

    let (_, body) = h.get("/api/v1/opportunities?stage=shortlisted").await;
    let list = body["opportunities"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], id.as_str());
    let (_, body) = h.get("/api/v1/opportunities?new=true").await;
    assert_eq!(body["opportunities"].as_array().unwrap().len(), 1);
    let (_, body) = h.get("/api/v1/opportunities?q=platform").await;
    assert_eq!(body["opportunities"][0]["title"], "Platform Engineer");
    let (_, body) = h.get("/api/v1/opportunities?q=%25").await;
    assert_eq!(
        body["opportunities"].as_array().unwrap().len(),
        0,
        "% is literal"
    );

    // Clearing a note.
    let (_, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "notes": null }),
        )
        .await;
    assert_eq!(body["notes"], json!(null));
    assert_eq!(body["next_step"], "Tailor the CV", "left out, kept");

    // Not for me: hidden from the pipeline, still there.
    let (_, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "dismissed": true }),
        )
        .await;
    assert_eq!(body["dismissed"], true);
    let (_, body) = h.get("/api/v1/opportunities").await;
    assert_eq!(body["opportunities"].as_array().unwrap().len(), 1);

    let (status, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "title": "Something else" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "not_editable");
    let (status, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "stage": "hired" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_opportunity");
    let (status, _) = h.get("/api/v1/opportunities?stage=hired").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn someone_elses_opportunity_is_not_found(db: PgPool) {
    let h = Harness::new(db.clone());
    seed(&h).await;
    let (_, body) = h.get("/api/v1/opportunities").await;
    let id = body["opportunities"][0]["id"].as_str().unwrap().to_owned();

    let other = Harness::new(db);
    assert!(other.account != h.account);
    let (status, _) = other.get(&format!("/api/v1/opportunities/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = other
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "stage": "closed" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = other.get("/api/v1/opportunities").await;
    assert_eq!(body["opportunities"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_greenhouse_link_is_read_from_the_board(db: PgPool) {
    let fake = Arc::new(Fake::default());
    *fake.posting.lock().unwrap() = Some(Posting {
        source: "greenhouse".into(),
        source_ref: "exampleco:4012345".into(),
        title: "Staff Engineer".into(),
        company: "Example Co".into(),
        location: Some("Lisbon, Portugal".into()),
        work_mode: Some("hybrid".into()),
        url: "https://boards.greenhouse.io/exampleco/jobs/4012345".into(),
        description: Some("Build things.".into()),
        ..Posting::default()
    });
    let h = Harness::as_account(db, Uuid::now_v7(), fake);
    let link = "https://boards.greenhouse.io/exampleco/jobs/4012345#apply";
    let (status, body) = h
        .json("POST", "/api/v1/opportunities", &json!({ "url": link }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["source"], "greenhouse");
    assert_eq!(body["title"], "Staff Engineer");
    assert_eq!(body["company"], "Example Co");
    assert_eq!(body["location"], "Lisbon, Portugal");
    assert_eq!(body["work_mode"], "hybrid");
    assert_eq!(body["match"], json!(null));
    assert_eq!(body["editable"], false);
    assert_eq!(
        h.fake.read.lock().unwrap().as_slice(),
        ["https://boards.greenhouse.io/exampleco/jobs/4012345"],
        "without its fragment"
    );

    // The same posting again: the one already there.
    let (status, again) = h
        .json("POST", "/api/v1/opportunities", &json!({ "url": link }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], body["id"]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_link_to_another_site_is_kept_as_a_link(db: PgPool) {
    let h = Harness::new(db);
    let (status, body) = h
        .json(
            "POST",
            "/api/v1/opportunities",
            &json!({ "url": "https://careers.example.org/roles/42", "title": " Designer " }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["source"], "link");
    assert_eq!(body["title"], "Designer");
    assert_eq!(body["company"], "");
    assert_eq!(body["editable"], true);
    let id = body["id"].as_str().unwrap().to_owned();
    let (status, body) = h
        .json(
            "PATCH",
            &format!("/api/v1/opportunities/{id}"),
            &json!({ "company": "Example Org", "location": "Porto" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["company"], "Example Org");
    assert_eq!(body["location"], "Porto");

    for url in [
        "ftp://example.org/x",
        "not a link",
        "javascript:alert(1)",
        "https://user:secret@example.org/",
        "",
    ] {
        let (status, body) = h
            .json("POST", "/api/v1/opportunities", &json!({ "url": url }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert_eq!(body["code"], "invalid_url");
    }
    let long = format!("https://example.org/{}", "a".repeat(2_000));
    let (status, _) = h
        .json("POST", "/api/v1/opportunities", &json!({ "url": long }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_match_that_fails_a_requirement_is_hidden_with_its_reason(db: PgPool) {
    let h = Harness::new(db);
    let kept = Found {
        eligibility: Some(store::Eligibility {
            remote: "yes".into(),
            open_to_candidate: "yes".into(),
            region_note: Some("EU".into()),
            working_languages: vec!["en".into()],
        }),
        ..found("exampleco:1", "Staff Engineer")
    };
    store::add_found(
        &h.db,
        &Default::default(),
        h.account,
        vec![kept, found("exampleco:2", "Platform Engineer")],
        Utc::now(),
    )
    .await
    .unwrap();
    // Found before the rules, then checked: remote only from Germany or Austria.
    let unchecked = store::unchecked(&h.db, 30).await.unwrap();
    assert_eq!(unchecked.len(), 1);
    let hidden_id = unchecked[0].id;
    store::set_eligibility(
        &h.db,
        h.account,
        hidden_id,
        store::Eligibility {
            remote: "yes".into(),
            open_to_candidate: "no".into(),
            region_note: Some("Germany or Austria only".into()),
            working_languages: vec!["en".into()],
        },
        Some("not_open_to_location"),
    )
    .await
    .unwrap();

    // Out of the pipeline and of the new matches.
    let (status, body) = h.get("/api/v1/opportunities").await;
    assert_eq!(status, StatusCode::OK);
    let list = body["opportunities"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["title"], "Staff Engineer");
    assert_eq!(list[0]["hidden_reason"], serde_json::Value::Null);
    let (_, body) = h.get("/api/v1/opportunities?new=true").await;
    assert_eq!(body["opportunities"].as_array().unwrap().len(), 1);

    // Listed on its own, with the reason.
    let (status, body) = h.get("/api/v1/opportunities?hidden=true").await;
    assert_eq!(status, StatusCode::OK);
    let list = body["opportunities"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["hidden_reason"], "not_open_to_location");
    assert_eq!(list[0]["region_note"], "Germany or Austria only");
    assert_eq!(list[0]["new"], false);

    // The peek shows the facts.
    let (status, body) = h.get(&format!("/api/v1/opportunities/{hidden_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["hidden_reason"], "not_open_to_location");
    assert_eq!(body["facts"]["remote"], "yes");
    assert_eq!(body["facts"]["open_to_candidate"], "no");
    assert_eq!(body["facts"]["working_languages"], json!(["en"]));
    let (_, body) = h.get("/api/v1/opportunities?hidden=true&new=true").await;
    assert!(body["opportunities"].as_array().unwrap().is_empty());
}
