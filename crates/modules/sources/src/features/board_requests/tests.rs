use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    SourcesConfig,
    testkit::{Fake, Harness, boards_config, today},
};

const OWNER: &str = "owner@example.com";

async fn harness(db: PgPool, owner: bool) -> Harness {
    let fake = Fake::lisbon_remote_only();
    fake.signed_in_as(if owner { OWNER } else { "someone@example.com" });
    let config = SourcesConfig {
        board_owners: Some(vec![OWNER.into()]),
        ..boards_config()
    };
    Harness::new(db, config, None, fake).await
}

/// A board that reads: a feed at `/api`, a check whose robots rules disallow
/// `disallow`, and `blockers` from the research.
async fn board(db: &PgPool, slug: &str, status: &str, disallow: &str, blockers: Value) -> Uuid {
    let id = Uuid::now_v7();
    let report = json!({
        "checked_at": today(),
        "robots": {
            "host": format!("{slug}.example"),
            "found": true,
            "rules": [{ "allow": false, "pattern": disallow }],
            "fingerprint": "x"
        },
        "terms_sentences": [],
        "sample": [],
        "flags": []
    });
    let config = json!({
        "endpoint": format!("https://{slug}.example/api"),
        "fields": { "title": "/title", "url": "/url" }
    });
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, config, terms, report, blockers, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'json_api', 'high', $6, 'discovery', $7, '{}', $8, $9, $10,
                 $10)",
    )
    .bind(id)
    .bind(slug)
    .bind(slug.to_uppercase())
    .bind(format!("https://{slug}.example"))
    .bind(format!("{slug}.example"))
    .bind(status)
    .bind(sqlx::types::Json(config))
    .bind(sqlx::types::Json(report))
    .bind(sqlx::types::Json(blockers))
    .bind(today())
    .execute(db)
    .await
    .unwrap();
    id
}

fn terms_forbid() -> Value {
    json!([{ "code": "terms_forbid", "detail": "You may not scrape this site." }])
}

async fn send(h: &Harness, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    h.send(request).await
}

#[sqlx::test(migrations = "../../../migrations")]
async fn anyone_but_an_owner_gets_a_404(db: PgPool) {
    let h = harness(db, false).await;
    let id = board(&h.db, "waits", "candidate", "/private", terms_forbid()).await;
    for (method, uri, body) in [
        ("GET", "/api/v1/boards/admin".to_owned(), None),
        ("GET", format!("/api/v1/boards/{id}/permission-email"), None),
        (
            "POST",
            format!("/api/v1/boards/{id}/permission/asked"),
            None,
        ),
        (
            "POST",
            format!("/api/v1/boards/{id}/permission/answer"),
            Some(json!({ "answer": "yes" })),
        ),
    ] {
        let (status, problem) = send(&h, method, &uri, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {problem}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_overview_counts_and_says_what_each_board_waits_for(db: PgPool) {
    let h = harness(db, true).await;
    board(&h.db, "reads", "approved", "/private", json!([])).await;
    board(&h.db, "stopped", "paused", "/private", json!([])).await;
    board(&h.db, "refused", "declined", "/private", json!([])).await;
    let asks = board(&h.db, "asks", "candidate", "/private", terms_forbid()).await;
    let ready = board(&h.db, "ready", "candidate", "/private", json!([])).await;
    board(&h.db, "forbidden", "candidate", "/api", json!([])).await;
    // No feed address yet: the app is still looking.
    let looking = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, created_at, updated_at)
         VALUES ($1, 'looking', 'Looking', 'https://looking.example', 'looking.example',
                 'json_api', 'low', 'candidate', 'discovery', $2, $2)",
    )
    .bind(looking)
    .bind(today())
    .execute(&h.db)
    .await
    .unwrap();
    let (status, _) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{asks}/permission/asked"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, overview) = h.get("/api/v1/boards/admin").await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    let totals = &overview["totals"];
    assert_eq!(totals["reading_sites"], 1);
    assert_eq!(totals["waiting"], 2);
    assert_eq!(totals["checking"], 1);
    // One declined, one whose robots rules forbid the feed.
    assert_eq!(totals["not_allowed"], 2);
    assert_eq!(totals["paused"], 1);
    let stage = |id: Uuid| {
        overview["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id.to_string())
            .cloned()
            .unwrap()
    };
    let asks = stage(asks);
    assert_eq!(asks["stage"], "waiting");
    assert_eq!(asks["need"], "permission");
    assert_eq!(asks["detail"], "You may not scrape this site.");
    assert!(asks["asked_at"].is_string());
    let ready = stage(ready);
    assert_eq!(
        (ready["stage"].as_str(), ready["need"].as_str()),
        (Some("waiting"), Some("approve"))
    );
    assert_eq!(stage(looking)["stage"], "checking");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_draft_names_the_board_and_what_the_app_promises(db: PgPool) {
    let h = harness(db, true).await;
    let id = board(&h.db, "waits", "candidate", "/private", terms_forbid()).await;
    let (status, email) = h
        .get(&format!("/api/v1/boards/{id}/permission-email"))
        .await;
    assert_eq!(status, StatusCode::OK, "{email}");
    assert_eq!(email["to"], Value::Null);
    assert_eq!(email["subject"], "May Applicamp show jobs from WAITS?");
    let body = email["body"].as_str().unwrap();
    for part in [
        "WAITS",
        "waits.example",
        "link back",
        "at most once an hour",
        "never repost",
        "drop jobs as soon as they close",
        "Applicamp",
    ] {
        assert!(body.contains(part), "{part}: {body}");
    }
    assert_eq!(email["asked_at"], Value::Null);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn yes_after_asking_approves_the_board_with_a_note(db: PgPool) {
    let h = harness(db, true).await;
    let id = board(&h.db, "waits", "candidate", "/private", terms_forbid()).await;
    let answer = json!({ "answer": "yes" });
    let uri = format!("/api/v1/boards/{id}/permission/answer");
    // Nothing was asked yet.
    let (status, problem) = send(&h, "POST", &uri, Some(answer.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "not_asked");

    let (status, email) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/asked"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{email}");
    let first = email["asked_at"].clone();
    assert!(first.is_string());
    // Asking twice keeps the first date.
    let (_, again) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/asked"),
        None,
    )
    .await;
    assert_eq!(again["asked_at"], first);

    let (status, detail) = send(&h, "POST", &uri, Some(answer)).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["board"]["status"], "approved");
    let (note, said): (String, String) =
        sqlx::query_as("SELECT note, answer FROM sources.board_requests WHERE board_id = $1")
            .bind(id)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(said, "yes");
    assert!(note.starts_with("They said yes on "), "{note}");
    // Answered once: the same answer again has nothing to answer.
    let (status, _) = send(&h, "POST", &uri, Some(json!({ "answer": "yes" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn no_declines_the_board_with_the_reason(db: PgPool) {
    let h = harness(db, true).await;
    let id = board(&h.db, "waits", "candidate", "/private", terms_forbid()).await;
    send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/asked"),
        None,
    )
    .await;
    let (status, detail) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/answer"),
        Some(json!({ "answer": "no", "note": "Not for apps." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["board"]["status"], "declined");
    let reason = detail["board"]["decline_reason"].as_str().unwrap();
    assert!(reason.starts_with("They said no on "), "{reason}");
    assert!(reason.ends_with("Not for apps."), "{reason}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn yes_does_not_override_robots_rules(db: PgPool) {
    let h = harness(db, true).await;
    let id = board(&h.db, "walled", "candidate", "/api", terms_forbid()).await;
    send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/asked"),
        None,
    )
    .await;
    let (status, problem) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/permission/answer"),
        Some(json!({ "answer": "yes" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "robots_disallowed");
    // Nothing recorded: it can still be answered later.
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sources.board_requests WHERE board_id = $1 AND answered_at IS NULL",
    )
    .bind(id)
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(open, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_decline_can_be_taken_back(db: PgPool) {
    let h = harness(db, true).await;
    let waits = board(&h.db, "waits", "candidate", "/private", terms_forbid()).await;
    let uri = format!("/api/v1/boards/{waits}/reopen");
    // Only a declined board.
    let (status, _) = send(&h, "POST", &uri, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    send(
        &h,
        "POST",
        &format!("/api/v1/boards/{waits}/decline"),
        Some(json!({ "reason": "No." })),
    )
    .await;
    let (status, detail) = send(&h, "POST", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["board"]["status"], "candidate");
    assert_eq!(detail["board"]["decline_reason"], Value::Null);
    // One that was read before goes back to being read.
    let read = board(&h.db, "read", "approved", "/private", json!([])).await;
    sqlx::query(
        "UPDATE sources.job_boards SET approved_at = now(), approved_by = 'owner' WHERE id = $1",
    )
    .bind(read)
    .execute(&h.db)
    .await
    .unwrap();
    send(
        &h,
        "POST",
        &format!("/api/v1/boards/{read}/decline"),
        Some(json!({ "reason": "No." })),
    )
    .await;
    let (_, detail) = send(&h, "POST", &format!("/api/v1/boards/{read}/reopen"), None).await;
    assert_eq!(detail["board"]["status"], "approved");
}
