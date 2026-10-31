//! Facts from an answer about a job, against a fake model. All data is
//! invented.

use applicamp_kernel::Error;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;

use super::{propose, user_message};
use crate::{
    schema::Work,
    testkit::{Harness, fake_model, gateway},
};

const ANSWER: &str =
    "I ran the Hotwire migration at Example Org. 3 of us moved 40 screens in 5 weeks.";

fn profile() -> Value {
    json!({
        "basics": { "name": "Alex Demo" },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02" },
        ],
    })
}

fn model_facts() -> Value {
    json!({ "facts": [
        { "text": "Ran the Hotwire migration", "quote": "I ran the Hotwire migration",
          "area": "your_part" },
        { "text": "Moved 40 screens in 5 weeks", "quote": "3 of us moved 40 screens in 5 weeks",
          "area": "results" },
        // The quote was never said.
        { "text": "Cut page weight by 60%", "quote": "we cut page weight by 60%", "area": "results" },
        // A real quote, but the text claims 90 screens.
        { "text": "Moved 90 screens", "quote": "3 of us moved 40 screens in 5 weeks",
          "area": "results" },
    ]})
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_answer_about_a_job_becomes_checked_proposals_to_keep(db: PgPool) {
    let server = fake_model(&model_facts()).await;
    let ai = gateway(&server, None);
    let h = Harness::new(db.clone(), Some(ai.clone()));
    let (status, saved) = h.json("PUT", "/api/v1/profile", &profile()).await;
    assert_eq!(status, 200, "{saved}");

    let answered = propose(
        &db,
        &ai,
        h.account,
        Utc::now(),
        0,
        "They are moving from React to Hotwire (their blog, Mar 2026). Have you worked with it?",
        ANSWER,
        Some("en"),
    )
    .await
    .unwrap();
    assert_eq!(answered.position, 0);
    assert_eq!(answered.role.company.as_deref(), Some("Example Org"));
    assert_eq!(answered.proposals.len(), 2, "the two checked facts");
    assert_eq!(answered.dropped, 2, "the made-up quote and the claim");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "one small call"
    );

    // They wait on the position in Grill me, ready to keep.
    let (_, view) = h.get("/api/v1/profile/grill/0").await;
    assert_eq!(view["proposals"].as_array().unwrap().len(), 2);
    assert_eq!(view["kept"], 0);

    let id = answered.proposals[0].id;
    let (status, kept) = h
        .json(
            "POST",
            &format!("/api/v1/profile/grill/proposals/{id}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(status, 200, "{kept}");
    assert_eq!(kept["kept"], 1);
    let (_, saved) = h.get("/api/v1/profile").await;
    assert_eq!(
        saved["profile"]["work"][0]["facts"][0]["text"],
        "Ran the Hotwire migration"
    );
    assert!(saved["profile"]["work"][1]["facts"].is_null());

    // The same answer again proposes nothing new: it is kept or waiting.
    let again = propose(&db, &ai, h.account, Utc::now(), 0, "Q?", ANSWER, None)
        .await
        .unwrap();
    assert!(again.proposals.is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_missing_position_or_a_bad_answer_calls_nothing(db: PgPool) {
    let server = fake_model(&model_facts()).await;
    let ai = gateway(&server, None);
    let h = Harness::new(db.clone(), Some(ai.clone()));
    h.json("PUT", "/api/v1/profile", &profile()).await;

    let err = propose(&db, &ai, h.account, Utc::now(), 7, "Q?", ANSWER, None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound));
    let err = propose(&db, &ai, h.account, Utc::now(), 0, "Q?", "   ", None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::BadRequest("invalid_answer", _)));
    let long = "x".repeat(2_001);
    let err = propose(&db, &ai, h.account, Utc::now(), 0, "Q?", &long, None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::BadRequest("invalid_answer", _)));
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[test]
fn the_model_sees_the_position_the_question_and_the_answer_but_not_the_place() {
    let work = Work {
        name: Some("Example Org".into()),
        position: Some("Senior Engineer".into()),
        location: Some("Lisbon".into()),
        ..Work::default()
    };
    let message = user_message(
        &work,
        "Do you know Hotwire?",
        "Yes, 2 years.",
        Some("pt-PT"),
    );
    assert!(message.contains("<language>pt-PT</language>"));
    assert!(message.contains("Example Org") && message.contains("Do you know Hotwire?"));
    assert!(!message.contains("Lisbon"));
    assert!(message.ends_with("<answer>\nYes, 2 years.\n</answer>"));
}
