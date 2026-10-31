use axum::{body::Body, http::Request};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    RunsDeps, probe,
    store::{self, Layer, Level, NewEvent},
    testkit::{Harness, scripted},
};

fn event(layer: Layer, level: Level, seq: Option<i64>, message: &str) -> NewEvent {
    NewEvent {
        layer,
        level,
        agent_seq: seq,
        operation: None,
        tool_name: None,
        message: message.to_owned(),
        data: None,
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_check_is_queued_and_listed(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (status, run) = harness
        .json("POST", "/api/v1/runs", &json!({ "kind": "runs.probe" }))
        .await;
    assert_eq!(status.as_u16(), 201, "{run}");
    assert_eq!(run["kind"], probe::KIND);
    assert_eq!(run["status"], "queued");
    assert_eq!(run["outcome"], serde_json::Value::Null);

    let (status, list) = harness.get("/api/v1/runs").await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(list["available"], true);
    assert_eq!(list["startable"], json!(["runs.probe"]));
    assert_eq!(list["runs"][0]["id"], run["id"]);

    let (status, got) = harness
        .get(&format!("/api/v1/runs/{}", run["id"].as_str().unwrap()))
        .await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(got["id"], run["id"]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn only_startable_kinds_start_and_only_with_a_runtime(db: PgPool) {
    let harness = Harness::new(db.clone(), scripted("true"));
    let (status, body) = harness
        .json("POST", "/api/v1/runs", &json!({ "kind": "runs.nothing" }))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (400, Some("unknown_kind"))
    );
    let (status, _) = harness
        .json(
            "POST",
            "/api/v1/runs",
            &json!({ "kind": "runs.probe", "x": 1 }),
        )
        .await;
    assert_eq!(status.as_u16(), 400);

    let off = Harness::with_deps(db, RunsDeps::default());
    let (status, body) = off
        .json("POST", "/api/v1/runs", &json!({ "kind": "runs.probe" }))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (503, Some("sandboxes_unavailable"))
    );
    let (_, list) = off.get("/api/v1/runs").await;
    assert_eq!(list["available"], false);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn at_most_three_wait(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    for _ in 0..3 {
        let (status, _) = harness
            .json("POST", "/api/v1/runs", &json!({ "kind": "runs.probe" }))
            .await;
        assert_eq!(status.as_u16(), 201);
    }
    let (status, body) = harness
        .json("POST", "/api/v1/runs", &json!({ "kind": "runs.probe" }))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (429, Some("too_many_runs"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn someone_elses_run_is_not_found(db: PgPool) {
    let harness = Harness::new(db.clone(), scripted("true"));
    let (theirs, _) = harness.going(Uuid::now_v7(), &probe::kind()).await;
    for uri in [
        format!("/api/v1/runs/{theirs}"),
        format!("/api/v1/runs/{theirs}/events"),
        format!("/api/v1/runs/{theirs}/events/stream"),
    ] {
        let (status, _) = harness.get(&uri).await;
        assert_eq!(status.as_u16(), 404, "{uri}");
    }
    let (status, _) = harness
        .json("POST", &format!("/api/v1/runs/{theirs}/cancel"), &json!({}))
        .await;
    assert_eq!(status.as_u16(), 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_waiting_run_is_stopped_at_once_and_an_ended_one_cannot_be(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (_, run) = harness
        .json("POST", "/api/v1/runs", &json!({ "kind": "runs.probe" }))
        .await;
    let id = run["id"].as_str().unwrap();
    let (status, body) = harness
        .json("POST", &format!("/api/v1/runs/{id}/cancel"), &json!({}))
        .await;
    assert_eq!(status.as_u16(), 202);
    assert_eq!(body["status"], "finished");
    assert_eq!(body["outcome"], "cancelled");
    assert_eq!(harness.finished().await.len(), 1);
    let (status, body) = harness
        .json("POST", &format!("/api/v1/runs/{id}/cancel"), &json!({}))
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (409, Some("run_finished"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_runs_events_page_by_layer(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (run, _) = harness.going(harness.account, &probe::kind()).await;
    let now = Utc::now();
    for event in [
        event(Layer::Agent, Level::Info, Some(1), "Read its context."),
        event(
            Layer::Sandbox,
            Level::Deny,
            None,
            "NET:OPEN [MED] DENIED applicamp-agent -> example.com:443",
        ),
        event(
            Layer::Agent,
            Level::Deny,
            Some(2),
            "example.com is blocked.",
        ),
    ] {
        assert!(
            store::insert_event(&harness.db, run, now, &event)
                .await
                .unwrap()
        );
    }
    let (status, body) = harness.get(&format!("/api/v1/runs/{run}/events")).await;
    assert_eq!(status.as_u16(), 200);
    let seqs: Vec<i64> = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["seq"].as_i64().unwrap())
        .collect();
    assert_eq!(seqs, [1, 2, 3]);
    let (_, body) = harness
        .get(&format!("/api/v1/runs/{run}/events?layer=sandbox"))
        .await;
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["events"][0]["level"], "deny");
    let (_, body) = harness
        .get(&format!("/api/v1/runs/{run}/events?after=2"))
        .await;
    assert_eq!(body["events"][0]["seq"], 3);
    let (status, _) = harness
        .get(&format!("/api/v1/runs/{run}/events?layer=other"))
        .await;
    assert_eq!(status.as_u16(), 400);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_live_stream_sends_the_events_then_the_end(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (run, _) = harness.going(harness.account, &probe::kind()).await;
    let now = Utc::now();
    for seq in 1..=3 {
        store::insert_event(
            &harness.db,
            run,
            now,
            &event(Layer::Agent, Level::Info, Some(seq), "step"),
        )
        .await
        .unwrap();
    }
    store::finish(
        &harness.db,
        &harness.state.events,
        run,
        applicamp_kernel::agent::Outcome::Succeeded,
        None,
        now,
    )
    .await
    .unwrap();
    let request = Request::get(format!("/api/v1/runs/{run}/events/stream"))
        .header("last-event-id", "1")
        .body(Body::empty())
        .unwrap();
    let (status, body) = harness.raw(request).await;
    assert_eq!(status.as_u16(), 200);
    assert!(
        !body.contains("id: 1\n"),
        "resumes after Last-Event-ID: {body}"
    );
    assert!(body.contains("event: run_event\n"), "{body}");
    assert!(
        body.contains("id: 2\n") && body.contains("id: 3\n"),
        "{body}"
    );
    let end = body.find("event: end").expect("an end frame");
    assert!(end > body.find("id: 3").unwrap());
    assert!(body[end..].contains("\"outcome\":\"succeeded\""), "{body}");
}
