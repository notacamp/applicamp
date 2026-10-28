use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    ports::{ProfileStatus, SearchLine},
    store::{self, Found},
    testkit::{Fake, Harness},
};

use super::{NextStep, next_step};

fn status(profile_done: bool, goals_answered: u32) -> ProfileStatus {
    ProfileStatus {
        profile_done,
        strength_hint: None,
        no_skills: false,
        goals_answered,
        goals_total: 11,
        strength: 0,
        next_action: None,
        questions_left: None,
    }
}

#[test]
fn the_next_step_follows_where_they_are() {
    assert_eq!(
        next_step(&status(false, 3), false, 0),
        NextStep::SetUpProfile
    );
    assert_eq!(next_step(&status(true, 0), false, 0), NextStep::AnswerGoals);
    assert_eq!(next_step(&status(true, 2), false, 0), NextStep::Search);
    assert_eq!(
        next_step(&status(true, 2), true, 4),
        NextStep::ReviewMatches
    );
    assert_eq!(next_step(&status(true, 2), true, 0), NextStep::None);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn profile_and_goals_done_before_any_search_offers_search_now(db: PgPool) {
    let fake = Arc::new(Fake::default());
    *fake.status.lock().unwrap() = ProfileStatus {
        strength_hint: Some("add_skills".into()),
        no_skills: true,
        strength: 46,
        next_action: Some("improve".into()),
        questions_left: Some(9),
        ..status(true, 3)
    };
    let h = Harness::as_account(db, Uuid::now_v7(), Arc::clone(&fake));
    let (code, body) = h.get("/api/v1/today").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["next_step"], "search");
    assert_eq!(body["profile"]["done"], true);
    assert_eq!(body["profile"]["strength_hint"], "add_skills");
    assert_eq!(body["profile"]["no_skills"], true);
    assert_eq!(body["goals"]["answered"], 3);
    assert_eq!(body["goals"]["total"], 11);
    assert_eq!(body["profile_strength"], 46);
    assert_eq!(body["next_profile_action"], "improve");
    assert_eq!(body["profile_questions"], 9);
    assert_eq!(body["latest_search"], serde_json::Value::Null);

    // After a search that found two: review them.
    store::add_found(
        &h.db,
        &Default::default(),
        h.account,
        vec![
            Found {
                source: "lever".into(),
                source_ref: "1".into(),
                title: "Engineer".into(),
                url: "https://example.com/1".into(),
                match_score: 70,
                ..Found::default()
            },
            Found {
                source: "lever".into(),
                source_ref: "2".into(),
                title: "Engineer".into(),
                url: "https://example.com/2".into(),
                match_score: 90,
                ..Found::default()
            },
        ],
        Utc::now(),
    )
    .await
    .unwrap();
    *fake.latest.lock().unwrap() = Some(SearchLine {
        id: Uuid::now_v7(),
        status: "finished".into(),
        trigger: "nightly".into(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        fetched: 412,
        added: 2,
        stopped: None,
    });
    let (_, body) = h.get("/api/v1/today").await;
    assert_eq!(body["next_step"], "review_matches");
    assert_eq!(body["new_matches"], 2);
    assert_eq!(body["latest_search"]["fetched"], 412);
    assert_eq!(body["latest_search"]["trigger"], "nightly");
    assert_eq!(body["latest_search"]["status"], "finished");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_a_profile_today_asks_for_one(db: PgPool) {
    let h = Harness::new(db);
    let (_, body) = h.get("/api/v1/today").await;
    assert_eq!(body["next_step"], "set_up_profile");
    assert_eq!(body["new_matches"], 0);
}
