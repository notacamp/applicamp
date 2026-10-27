use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::TimeDelta;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    SourcesConfig,
    testkit::{Fake, Harness, boards_config, today},
};

const OWNER: &str = "owner@example.com";

fn owners_config() -> SourcesConfig {
    SourcesConfig {
        board_owners: Some(vec![OWNER.into()]),
        ..boards_config()
    }
}

async fn harness(db: PgPool, owner: bool) -> Harness {
    let fake = Fake::lisbon_remote_only();
    if owner {
        fake.signed_in_as(OWNER);
    } else {
        fake.signed_in_as("someone@example.com");
    }
    Harness::new(db, owners_config(), None, fake).await
}

/// A board straight into the registry: `slug`, `status`, `country`, its
/// config and report.
async fn board(
    db: &PgPool,
    slug: &str,
    status: &str,
    country: Option<&str>,
    config: Value,
    report: Option<Value>,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, country, config, terms, report, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'json_api', 'high', $6, 'discovery', $7, $8, '{}', $9, $10,
                 $10)",
    )
    .bind(id)
    .bind(slug)
    .bind(slug.to_uppercase())
    .bind(format!("https://{slug}.example"))
    .bind(format!("{slug}.example"))
    .bind(status)
    .bind(country)
    .bind(sqlx::types::Json(config))
    .bind(report.map(sqlx::types::Json))
    .bind(today())
    .execute(db)
    .await
    .unwrap();
    id
}

/// `n` engineering jobs on `board` paying `eur` a year, open anywhere.
async fn listed(db: &PgPool, board: Uuid, n: usize, eur: Option<f64>) {
    for i in 0..n {
        sqlx::query(
            "INSERT INTO sources.board_jobs (board_id, source_ref, title, regions, engineering,
                salary_eur_year, posted_at, seen_at)
             VALUES ($1, $2, 'Senior Engineer', '{}', true, $3, $4, $4)",
        )
        .bind(board)
        .bind(format!("job-{i}"))
        .bind(eur)
        .bind(today() - TimeDelta::days(2))
        .execute(db)
        .await
        .unwrap();
    }
}

fn feed_config(slug: &str, path: &str) -> Value {
    json!({
        "endpoint": format!("https://{slug}.example{path}"),
        "fields": { "title": "/title", "url": "/url" }
    })
}

fn checked_report(slug: &str, disallow: &str) -> Value {
    json!({
        "checked_at": today(),
        "robots": {
            "host": format!("{slug}.example"),
            "found": true,
            "rules": [{ "allow": false, "pattern": disallow }],
            "fingerprint": "x"
        },
        "terms_sentences": ["You may not scrape this site."],
        "sample": [{ "title": "Senior Engineer", "company": "Example Co", "location": "Remote",
                     "url": "https://example.com/1", "regions": [] }],
        "flags": []
    })
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
async fn boards_are_ranked_by_average_salary(db: PgPool) {
    let h = harness(db, false).await;
    let rich = board(
        &h.db,
        "rich",
        "approved",
        None,
        feed_config("rich", "/api"),
        None,
    )
    .await;
    let modest = board(
        &h.db,
        "modest",
        "approved",
        None,
        feed_config("modest", "/api"),
        None,
    )
    .await;
    let swiss = board(&h.db, "swiss", "candidate", Some("CH"), json!({}), None).await;
    let portuguese = board(
        &h.db,
        "portuguese",
        "candidate",
        Some("PT"),
        json!({}),
        None,
    )
    .await;
    let few = board(&h.db, "few", "candidate", None, json!({}), None).await;
    board(&h.db, "silent", "approved", None, json!({}), None).await;
    listed(&h.db, rich, 12, Some(120_000.0)).await;
    listed(&h.db, modest, 12, Some(80_000.0)).await;
    listed(&h.db, swiss, 3, Some(70_000.0)).await;
    listed(&h.db, portuguese, 2, Some(90_000.0)).await;
    listed(&h.db, few, 4, Some(60_000.0)).await;

    let (status, list) = h.get("/api/v1/boards").await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["can_manage"], false);
    let order: Vec<&str> = list["boards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["slug"].as_str().unwrap())
        .collect();
    // €120k, Switzerland's benchmark (€120 651), €80k, few salaries (€60k),
    // Portugal's benchmark (€36 066), no salary data last.
    assert_eq!(
        order,
        ["swiss", "rich", "modest", "few", "portuguese", "silent"]
    );
    let by = |slug: &str| {
        list["boards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["slug"] == slug)
            .cloned()
            .unwrap()
    };
    let rich = by("rich");
    assert_eq!(rich["ranking"]["basis"], "own");
    assert_eq!(rich["ranking"]["salaried_jobs"], 12);
    assert_eq!(rich["ranking"]["relevant_jobs"], 12);
    assert_eq!(by("few")["ranking"]["basis"], "few");
    let swiss = by("swiss");
    assert_eq!(swiss["ranking"]["basis"], "benchmark");
    assert_eq!(swiss["ranking"]["benchmark"]["code"], "CH");
    assert_eq!(by("silent")["ranking"]["basis"], "none");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn only_a_board_owner_decides(db: PgPool) {
    let h = harness(db, false).await;
    let id = board(
        &h.db,
        "candidate",
        "candidate",
        None,
        feed_config("candidate", "/api"),
        Some(checked_report("candidate", "/private")),
    )
    .await;
    for (method, uri, body) in [
        ("POST", format!("/api/v1/boards/{id}/approve"), None),
        (
            "POST",
            format!("/api/v1/boards/{id}/decline"),
            Some(json!({ "reason": "No." })),
        ),
        ("POST", format!("/api/v1/boards/{id}/pause"), None),
        ("POST", format!("/api/v1/boards/{id}/check"), None),
        (
            "PUT",
            format!("/api/v1/boards/{id}/config"),
            Some(json!({ "config": {} })),
        ),
    ] {
        let (status, problem) = send(&h, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {problem}");
        assert_eq!(problem["code"], "not_board_owner");
    }
    // Everyone sees it, with its check.
    let (status, detail) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["can_manage"], false);
    assert_eq!(
        detail["report"]["terms_sentences"][0],
        "You may not scrape this site."
    );
    assert_eq!(detail["approvable"], true);
    let (status, _) = h.get(&format!("/api/v1/boards/{}", Uuid::now_v7())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn approving_needs_robots_rules_that_allow_the_feed(db: PgPool) {
    let h = harness(db, true).await;
    // Never checked: check first.
    let unchecked = board(
        &h.db,
        "unchecked",
        "candidate",
        None,
        feed_config("unchecked", "/api"),
        None,
    )
    .await;
    let (status, problem) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{unchecked}/approve"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "check_needed");

    // Its robots rules forbid the API: refused until the config says an
    // allowed path.
    let id = board(
        &h.db,
        "blocked",
        "candidate",
        None,
        feed_config("blocked", "/api/jobs"),
        Some(checked_report("blocked", "/api/")),
    )
    .await;
    let (_, detail) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(detail["approvable"], false);
    assert_eq!(detail["blocked"], "robots_disallowed");
    let (status, problem) = send(&h, "POST", &format!("/api/v1/boards/{id}/approve"), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "robots_disallowed");

    // A config that does not read is refused, with what is wrong.
    let (status, problem) = send(
        &h,
        "PUT",
        &format!("/api/v1/boards/{id}/config"),
        Some(json!({ "config": { "endpoint": "https://blocked.example/feed", "evil": true } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["code"], "invalid_config");

    let (status, detail) = send(
        &h,
        "PUT",
        &format!("/api/v1/boards/{id}/config"),
        Some(json!({ "config": feed_config("blocked", "/feed.json") })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["approvable"], true);
    let (status, detail) = send(&h, "POST", &format!("/api/v1/boards/{id}/approve"), None).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["board"]["status"], "approved");
    let edited: bool =
        sqlx::query_scalar("SELECT config_edited FROM sources.job_boards WHERE id = $1")
            .bind(id)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert!(edited, "the seed list never overwrites it");
    let read: Vec<String> = crate::job_boards::approved(&h.db)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.slug)
        .collect();
    assert_eq!(read, ["blocked"]);

    // Paused, then declined with a reason.
    let (status, detail) = send(&h, "POST", &format!("/api/v1/boards/{id}/pause"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["board"]["status"], "paused");
    let (status, _) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/decline"),
        Some(json!({ "reason": " " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a reason is needed");
    let (status, detail) = send(
        &h,
        "POST",
        &format!("/api/v1/boards/{id}/decline"),
        Some(json!({ "reason": "Its jobs are stale." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["board"]["status"], "declined");
    assert_eq!(detail["board"]["decline_reason"], "Its jobs are stale.");
    let (status, problem) = send(&h, "POST", &format!("/api/v1/boards/{id}/approve"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "not_waiting");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_suggested_board_waits_for_its_check(db: PgPool) {
    let fake = Fake::ready();
    fake.signed_in_as("someone@example.com");
    let h = Harness::new(
        db,
        SourcesConfig {
            allow_private_hosts: Some(false),
            ..owners_config()
        },
        None,
        fake,
    )
    .await;
    let suggest = |url: &str| json!({ "url": url });
    for bad in [
        "not a link",
        "http://jobs.example.org",
        "https://127.0.0.1/",
        "https://localhost/x",
    ] {
        let (status, problem) =
            send(&h, "POST", "/api/v1/boards/suggestions", Some(suggest(bad))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {problem}");
        assert_eq!(problem["code"], "invalid_url");
    }
    let (status, detail) = send(
        &h,
        "POST",
        "/api/v1/boards/suggestions",
        Some(suggest("https://www.jobs.example.org/remote?ref=x")),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{detail}");
    assert_eq!(detail["board"]["status"], "candidate");
    assert_eq!(detail["board"]["origin"], "suggested");
    assert_eq!(detail["board"]["slug"], "jobs-example-org");
    assert_eq!(detail["board"]["homepage"], "https://www.jobs.example.org/");
    let checks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM kernel.jobs WHERE kind = 'sources.check_board'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(checks, 1, "checked in the background");
    // The same site again: the board already there.
    let (status, again) = send(
        &h,
        "POST",
        "/api/v1/boards/suggestions",
        Some(suggest("https://jobs.example.org/")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["board"]["id"], detail["board"]["id"]);
    // Never read for jobs while it waits.
    assert!(crate::job_boards::approved(&h.db).await.unwrap().is_empty());
}
