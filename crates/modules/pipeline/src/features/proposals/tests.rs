//! Shortlist proposals over a real Postgres: invented jobs, fake neighbours.

use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{PER_DAY, shortlist};
use crate::{
    PipelineDeps,
    ports::{Autonomy, Neighbours},
    store::{self, Found},
    testkit::{Fake, Harness, Model},
};

fn found(n: u32, score: u8) -> Found {
    Found {
        source: "greenhouse".into(),
        source_ref: format!("exampleco:{n}"),
        title: format!("Engineer {n}"),
        company: "Example Co".into(),
        location: Some("Remote - Europe".into()),
        work_mode: Some("remote".into()),
        url: format!("https://example.com/jobs/{n}"),
        description: Some("Build things.".into()),
        match_score: score,
        ..Found::default()
    }
}

struct Stack {
    h: Harness,
    fake: Arc<Fake>,
    deps: PipelineDeps,
    _model: Model,
}

async fn stack(db: PgPool) -> Stack {
    let fake = Arc::new(Fake::default());
    let model = Model::new(json!({"facts": []}), json!({"wants": [], "questions": []})).await;
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    let neighbours: Arc<dyn Neighbours> = fake.clone();
    let deps = PipelineDeps {
        neighbours: Some(neighbours),
        ai: Some(model.gateway()),
    };
    store::add_found(
        &h.db,
        &Default::default(),
        h.account,
        vec![
            found(1, 92),
            found(2, 74),
            found(3, 88),
            found(4, 80),
            found(5, 76),
        ],
        Utc::now(),
    )
    .await
    .unwrap();
    Stack {
        h,
        fake,
        deps,
        _model: model,
    }
}

async fn proposed(h: &Harness) -> Vec<Value> {
    let (status, body) = h.get("/api/v1/opportunities?proposed=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["opportunities"].as_array().unwrap().clone()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_job_that_fits_well_is_proposed_best_first_until_it_moves_or_is_set_aside(db: PgPool) {
    let s = stack(db).await;
    let list = proposed(&s.h).await;
    let scores: Vec<i64> = list.iter().map(|o| o["match"].as_i64().unwrap()).collect();
    assert_eq!(scores, [92, 88, 80, 76], "74 is not enough");
    assert!(list.iter().all(|o| o["proposed"] == true));

    // Set aside: no longer proposed. Moved by the person: theirs.
    let first = list[0]["id"].as_str().unwrap().to_owned();
    let second = list[1]["id"].as_str().unwrap().to_owned();
    let (status, _) =
        s.h.json(
            "PATCH",
            &format!("/api/v1/opportunities/{first}"),
            &json!({ "dismissed": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) =
        s.h.json(
            "PATCH",
            &format!("/api/v1/opportunities/{second}"),
            &json!({ "stage": "shortlisted" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["shortlisted_by"], "person");
    assert_eq!(body["proposed"], false);
    assert_eq!(proposed(&s.h).await.len(), 2);

    // One a requirement rules out is no longer proposed.
    let third = proposed(&s.h).await[0]["id"].as_str().unwrap().to_owned();
    store::set_eligibility(
        &s.h.db,
        s.h.account,
        Uuid::parse_str(&third).unwrap(),
        store::Eligibility {
            remote: "yes".into(),
            open_to_candidate: "no".into(),
            region_note: None,
            working_languages: vec![],
            ..store::Eligibility::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(proposed(&s.h).await.len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn with_prepare_the_app_shortlists_the_best_three_a_day_and_prepares_them(db: PgPool) {
    let s = stack(db).await;
    *s.fake.autonomy.lock().unwrap() = Some(Autonomy::Prepare);
    let now = Utc::now();
    assert_eq!(
        shortlist(&s.deps, &s.h.db, s.h.account, now).await.unwrap(),
        3
    );
    let (_, body) = s.h.get("/api/v1/opportunities?stage=shortlisted").await;
    let mut shortlisted: Vec<(i64, String)> = body["opportunities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["match"].as_i64().unwrap(),
                o["shortlisted_by"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    shortlisted.sort_unstable();
    assert_eq!(
        shortlisted,
        [
            (80, "app".to_owned()),
            (88, "app".to_owned()),
            (92, "app".to_owned())
        ]
    );
    // Their preparation started, and says the app shortlisted them.
    let done: Vec<(String, Value)> = sqlx::query_as::<_, (String, sqlx::types::Json<Value>)>(
        "SELECT state, done FROM pipeline.preparations WHERE account_id = $1",
    )
    .bind(s.h.account)
    .fetch_all(&s.h.db)
    .await
    .unwrap()
    .into_iter()
    .map(|(state, done)| (state, done.0))
    .collect();
    assert_eq!(done.len(), 3);
    assert!(done.iter().all(|(state, _)| state == "queued"));
    assert!(done.iter().all(|(_, d)| d[0]["step"] == "shortlist"));
    // The day's three are used: the next search moves nothing.
    assert_eq!(
        shortlist(&s.deps, &s.h.db, s.h.account, now).await.unwrap(),
        0
    );
    assert_eq!(proposed(&s.h).await.len(), 1);
    assert_eq!(PER_DAY, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn with_find_only_nothing_moves(db: PgPool) {
    let s = stack(db).await;
    *s.fake.autonomy.lock().unwrap() = Some(Autonomy::FindOnly);
    assert_eq!(
        shortlist(&s.deps, &s.h.db, s.h.account, Utc::now())
            .await
            .unwrap(),
        0
    );
    assert_eq!(proposed(&s.h).await.len(), 4);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_daily_cv_cap_holds_the_app_back(db: PgPool) {
    let s = stack(db).await;
    let opp: Uuid = sqlx::query_scalar("SELECT id FROM pipeline.opportunities LIMIT 1")
        .fetch_one(&s.h.db)
        .await
        .unwrap();
    for n in 1..=crate::features::cv::CAP_PER_DAY {
        sqlx::query(
            "INSERT INTO pipeline.cv_drafts
                (id, account_id, opportunity_id, n, language, status, content, frame,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'en', 'discarded', '{}', '{}', now(), now())",
        )
        .bind(Uuid::now_v7())
        .bind(s.h.account)
        .bind(opp)
        .bind(i32::try_from(n).unwrap())
        .execute(&s.h.db)
        .await
        .unwrap();
    }
    assert_eq!(
        shortlist(&s.deps, &s.h.db, s.h.account, Utc::now())
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_posting_that_has_closed_is_never_shortlisted(db: PgPool) {
    let s = stack(db).await;
    *s.fake.autonomy.lock().unwrap() = Some(Autonomy::Prepare);
    // The best match closed yesterday.
    sqlx::query(
        "UPDATE pipeline.opportunities SET expires_at = now() - interval '1 day'
          WHERE match_score = 92",
    )
    .execute(&s.h.db)
    .await
    .unwrap();
    assert_eq!(
        shortlist(&s.deps, &s.h.db, s.h.account, Utc::now())
            .await
            .unwrap(),
        3
    );
    let mut scores: Vec<i32> = sqlx::query_scalar(
        "SELECT match_score FROM pipeline.opportunities WHERE shortlisted_by = 'app'",
    )
    .fetch_all(&s.h.db)
    .await
    .unwrap();
    scores.sort_unstable();
    assert_eq!(scores, [76, 80, 88]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn two_searches_ending_at_once_shortlist_three_at_most(db: PgPool) {
    let s = stack(db).await;
    *s.fake.autonomy.lock().unwrap() = Some(Autonomy::Prepare);
    let now = Utc::now();
    let (first, second) = tokio::join!(
        shortlist(&s.deps, &s.h.db, s.h.account, now),
        shortlist(&s.deps, &s.h.db, s.h.account, now),
    );
    assert_eq!(first.unwrap() + second.unwrap(), 3);
    let app: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.opportunities WHERE shortlisted_by = 'app'",
    )
    .fetch_one(&s.h.db)
    .await
    .unwrap();
    assert_eq!(app, 3);
}
