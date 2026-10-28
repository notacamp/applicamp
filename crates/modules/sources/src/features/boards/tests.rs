use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::TimeDelta;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use crate::{
    SourcesConfig,
    features::discovery::check_seeded,
    job_boards::{self, SeedBoard, Status},
    testkit::{Fake, Harness, boards_config, requests_to, today},
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
        (
            "PUT",
            format!("/api/v1/boards/{id}/key"),
            Some(json!({ "values": { "api_key": "x" } })),
        ),
        ("DELETE", format!("/api/v1/boards/{id}/key"), None),
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

/// The seed list's board `slug`, waiting for the owner, its site and its
/// API on the fake `site` (each API at `/<slug><its own path>`).
async fn seed_candidate(db: &PgPool, site: &MockServer, slug: &str) -> Uuid {
    let seed: Vec<SeedBoard> = job_boards::parse(job_boards::SEED)
        .unwrap()
        .into_iter()
        .filter(|b| b.slug == slug)
        .map(|mut board| {
            board.homepage = format!("{}/", site.uri()).parse().unwrap();
            if let Some(endpoint) = &board.config.endpoint {
                let fake = format!("{}/{}{}", site.uri(), board.slug, endpoint.path());
                board.config.endpoint = Some(fake.parse().unwrap());
            }
            board.config.min_interval_ms = None;
            board.status = Status::Candidate;
            board
        })
        .collect();
    assert_eq!(seed.len(), 1, "{slug}");
    job_boards::sync(db, &seed, today()).await.unwrap();
    sqlx::query_scalar("SELECT id FROM sources.job_boards WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .unwrap()
}

/// A board's site: robots rules that allow its API, and a homepage.
async fn a_site() -> MockServer {
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /admin/\n"),
        )
        .mount(&site)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("<html lang=\"en\"><body>Jobs.</body></html>", "text/html"),
        )
        .mount(&site)
        .await;
    site
}

fn codes(detail: &Value) -> Vec<String> {
    detail["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["code"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_seeded_candidate_is_approvable_after_its_check_at_start(db: PgPool) {
    let site = a_site().await;
    Mock::given(method("GET"))
        .and(path("/swissdevjobs/api/jobsLight"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../readers/fixtures/boards/swissdevjobs.json"),
            "application/json",
        ))
        .mount(&site)
        .await;
    let h = harness(db, true).await;
    let id = seed_candidate(&h.db, &site, "swissdevjobs").await;
    let (_, before) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(before["approvable"], false);
    assert_eq!(before["blocked"], "check_needed", "{before}");

    // At start: checked once, with its own config.
    assert_eq!(check_seeded(&h.deps, &h.db).await.unwrap(), 1);
    assert_eq!(
        check_seeded(&h.deps, &h.db).await.unwrap(),
        0,
        "never twice with the same config"
    );
    assert_eq!(requests_to(&site, "/swissdevjobs").await, 1);
    let (_, after) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(after["approvable"], true, "{after}");
    assert!(codes(&after).is_empty());
    assert_eq!(after["report"]["sample"].as_array().unwrap().len(), 3);
    assert_eq!(
        after["report"]["sample"][0]["url"],
        "https://swissdevjobs.ch/jobs/Example-AG-Senior-Backend-Engineer"
    );
    assert!(
        after["report"]["proposal"].is_null(),
        "its own config reads"
    );
    let (status, approved) = send(&h, "POST", &format!("/api/v1/boards/{id}/approve"), None).await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["board"]["status"], "approved");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_that_needs_a_key_says_how_to_get_one_and_is_checked_once_set(db: PgPool) {
    let site = a_site().await;
    Mock::given(method("GET"))
        .and(path("/itjobs-pt/job/list.json"))
        .and(query_param("api_key", "test-key-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total": 1,
            "results": [{ "id": 4242, "title": "Backend Developer",
                          "company": { "name": "Exemplo Lda" }, "body": "<p>Invented.</p>",
                          "publishedAt": "2026-10-27 10:00:00" }]
        })))
        .mount(&site)
        .await;
    let h = harness(db, true).await;
    let id = seed_candidate(&h.db, &site, "itjobs-pt").await;
    let uri = format!("/api/v1/boards/{id}");
    let (_, detail) = h.get(&uri).await;
    assert_eq!(codes(&detail)[0], "needs_key", "{detail}");
    let reason = detail["blockers"][0]["detail"].as_str().unwrap();
    assert!(
        reason.starts_with("Needs an API key from ITJobs.pt: Enter your email"),
        "{reason}"
    );
    assert_eq!(detail["blockers"][0]["source"], "https://www.itjobs.pt/api");
    assert_eq!(detail["key"]["set"], false);
    assert_eq!(detail["key"]["params"], json!(["api_key"]));

    // Checked at start without its API: never asked without the key.
    assert_eq!(check_seeded(&h.deps, &h.db).await.unwrap(), 1);
    assert_eq!(requests_to(&site, "/itjobs-pt").await, 0);
    let (_, detail) = h.get(&uri).await;
    assert!(
        detail["board"]["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().contains("needs a key")),
        "{detail}"
    );

    // A key that cannot be one, or a parameter it does not have.
    for (values, field) in [
        (json!({ "api_key": "has space" }), "`api_key`"),
        (json!({ "api_key": "" }), "`api_key`"),
        (json!({ "other": "x" }), "`other`"),
    ] {
        let (status, problem) = send(
            &h,
            "PUT",
            &format!("{uri}/key"),
            Some(json!({ "values": values })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
        assert_eq!(problem["code"], "invalid_key");
        assert!(
            problem["detail"].as_str().unwrap().contains(field),
            "{problem}"
        );
    }

    // Set: never shown again, kept sealed; the board says "key set".
    let (status, detail) = send(
        &h,
        "PUT",
        &format!("{uri}/key"),
        Some(json!({ "values": { "api_key": "test-key-1" } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["key"]["set"], true);
    assert!(!codes(&detail).contains(&"needs_key".to_owned()));
    assert!(!detail.to_string().contains("test-key-1"));
    let sealed: Vec<u8> =
        sqlx::query_scalar("SELECT api_key FROM sources.job_boards WHERE id = $1")
            .bind(id)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("test-key-1"));

    // Checked again, with its key: approvable.
    let (status, _) = send(&h, "POST", &format!("{uri}/check"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    h.work().await;
    let (_, detail) = h.get(&uri).await;
    assert_eq!(detail["approvable"], true, "{detail}");
    assert_eq!(
        detail["report"]["sample"][0]["url"],
        "https://www.itjobs.pt/oferta/4242"
    );
    let leaks: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM sources.job_boards
                  WHERE report::text LIKE '%test-key-1%' OR config::text LIKE '%test-key-1%')
              + (SELECT count(*) FROM sources.board_scans WHERE note LIKE '%test-key-1%')
              + (SELECT count(*) FROM sources.cache WHERE url LIKE '%test-key-1%')",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(leaks, 0, "the key is kept nowhere but sealed");

    // Removed: it needs one again.
    let (status, detail) = send(&h, "DELETE", &format!("{uri}/key"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(codes(&detail)[0], "needs_key");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_config_that_does_not_read_is_refused_naming_the_field(db: PgPool) {
    let h = harness(db, true).await;
    let id = board(
        &h.db,
        "edited",
        "candidate",
        None,
        feed_config("edited", "/api"),
        None,
    )
    .await;
    let uri = format!("/api/v1/boards/{id}/config");
    for (config, field) in [
        (
            json!({ "endpoint": "https://edited.example/api", "max_pages": "five" }),
            "`max_pages`",
        ),
        (
            json!({ "endpoint": "https://edited.example/api", "fields": { "title": "/t" } }),
            "`fields.url` is required",
        ),
        (json!({ "fields": {} }), "`endpoint` is required"),
        (
            json!({ "endpont": "https://edited.example/api" }),
            "`endpont` is not a config key",
        ),
    ] {
        let (status, problem) = send(&h, "PUT", &uri, Some(json!({ "config": config }))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
        assert_eq!(problem["code"], "invalid_config");
        assert!(
            problem["detail"].as_str().unwrap().contains(field),
            "{field}: {problem}"
        );
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn what_the_research_found_blocks_approval_and_is_never_read(db: PgPool) {
    let h = harness(db, true).await;
    let seed: Vec<SeedBoard> = job_boards::parse(job_boards::SEED)
        .unwrap()
        .into_iter()
        .filter(|b| b.slug == "landing-jobs")
        .collect();
    job_boards::sync(&h.db, &seed, today()).await.unwrap();
    let id: Uuid =
        sqlx::query_scalar("SELECT id FROM sources.job_boards WHERE slug = 'landing-jobs'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    let (_, detail) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(detail["board"]["status"], "candidate");
    assert_eq!(&codes(&detail)[..2], ["terms_forbid", "robots_disallowed"]);
    assert!(
        detail["blockers"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("scrape or copy the Platform")
    );
    assert_eq!(detail["blockers"][0]["source"], "https://landing.jobs/tos");
    let (status, problem) = send(&h, "POST", &format!("/api/v1/boards/{id}/approve"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(problem["code"], "terms_forbid");
    assert!(problem["detail"].as_str().unwrap().contains("scrape"));
    // Its site is not read at start.
    assert_eq!(check_seeded(&h.deps, &h.db).await.unwrap(), 0);
}

const SEEDED_DECLINES_WAIT: &str =
    include_str!("../../../../../../migrations/20261028120100_sources__seeded_declines_wait.sql");

#[sqlx::test(migrations = "../../../migrations")]
async fn boards_the_seed_list_declined_wait_for_the_owner_again(db: PgPool) {
    // As the first seed list left them, and one an owner declined.
    for (slug, host, origin, reason) in [
        (
            "landing-jobs",
            "landing.jobs",
            "seed",
            "Its robots.txt disallows /api/ and its terms forbid scraping.",
        ),
        (
            "the-muse",
            "themuse.com",
            "seed",
            "US-centric, and its listings are stale.",
        ),
        (
            "wellfound",
            "wellfound.com",
            "seed",
            "Its robots.txt disallows automated reading.",
        ),
        (
            "dice",
            "dice.com",
            "seed",
            "Its robots.txt disallows automated reading.",
        ),
        (
            "not-for-me",
            "not-for-me.example",
            "discovery",
            "Not for me.",
        ),
    ] {
        sqlx::query(
            "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority,
                status, origin, decline_reason, created_at, updated_at)
             VALUES ($1, $2, $2, $3, $4, 'json_api', 'low', 'declined', $5, $6, now(), now())",
        )
        .bind(Uuid::now_v7())
        .bind(slug)
        .bind(format!("https://{host}"))
        .bind(host)
        .bind(origin)
        .bind(reason)
        .execute(&db)
        .await
        .unwrap();
    }
    // The owner declined The Muse again themselves: theirs to keep.
    sqlx::query("UPDATE sources.job_boards SET decline_reason = 'Too US.' WHERE slug = 'the-muse'")
        .execute(&db)
        .await
        .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(SEEDED_DECLINES_WAIT)
            .execute(&db)
            .await
            .unwrap();
    }
    let rows = job_boards::all(&db).await.unwrap();
    let by = |slug: &str| rows.iter().find(|b| b.slug == slug).unwrap();
    for slug in ["landing-jobs", "wellfound", "dice"] {
        let row = by(slug);
        assert_eq!(row.status(), Status::Candidate, "{slug}");
        assert_eq!(row.decline_reason, None);
        assert!(!row.report().unwrap().notes.is_empty(), "{slug}");
        let codes: Vec<String> = row.blockers().into_iter().map(|b| b.code).collect();
        assert!(codes.contains(&"terms_forbid".to_owned()), "{slug}");
        assert!(codes.contains(&"robots_disallowed".to_owned()), "{slug}");
    }
    assert_eq!(
        by("the-muse").status(),
        Status::Declined,
        "the owner's decision"
    );
    assert_eq!(by("not-for-me").status(), Status::Declined);
    assert_eq!(
        by("not-for-me").decline_reason.as_deref(),
        Some("Not for me.")
    );
    // No board the owner never declined is shown as declined.
    let (status, list) = harness(db, false).await.get("/api/v1/boards").await;
    assert_eq!(status, StatusCode::OK);
    let declined: Vec<&str> = list["boards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["status"] == "declined")
        .map(|b| b["slug"].as_str().unwrap())
        .collect();
    assert_eq!(declined, ["not-for-me", "the-muse"]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_says_how_well_it_fits_your_top_skills(db: PgPool) {
    let fake =
        Fake::lisbon_remote_only().with_skills(&["Ruby on Rails", "React", "Postgres", "Go"]);
    fake.signed_in_as("someone@example.com");
    let h = Harness::new(db, owners_config(), None, fake).await;
    let id = board(&h.db, "stack", "candidate", None, json!({}), None).await;
    let words: [&[&str]; 5] = [
        &["senior", "engineer", "ruby", "rails"],
        &["engineer", "react"],
        &["engineer", "java"],
        &["engineer", "cobol"],
        // Only the fourth skill: not a top skill.
        &["engineer", "go"],
    ];
    for (i, w) in words.iter().enumerate() {
        sqlx::query(
            "INSERT INTO sources.board_jobs (board_id, source_ref, title, regions, engineering,
                posted_at, seen_at, words)
             VALUES ($1, $2, 'Engineer', '{}', true, $3, $3, $4)",
        )
        .bind(id)
        .bind(format!("job-{i}"))
        .bind(today() - TimeDelta::days(1))
        .bind(w.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>())
        .execute(&h.db)
        .await
        .unwrap();
    }
    // A job from before the words were kept is not counted.
    sqlx::query(
        "INSERT INTO sources.board_jobs (board_id, source_ref, title, regions, engineering,
            posted_at, seen_at)
         VALUES ($1, 'before-words', 'Engineer', '{}', true, $2, $2)",
    )
    .bind(id)
    .bind(today() - TimeDelta::days(1))
    .execute(&h.db)
    .await
    .unwrap();

    let (status, list) = h.get("/api/v1/boards").await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let fit = &list["boards"][0]["skills_fit"];
    assert_eq!(fit["jobs"], 5, "{list}");
    assert!((fit["share"].as_f64().unwrap() - 0.4).abs() < 1e-9, "{fit}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_skills_no_board_shows_a_fit(db: PgPool) {
    let h = harness(db, false).await;
    let id = board(&h.db, "stack", "candidate", None, json!({}), None).await;
    listed(&h.db, id, 3, None).await;
    let (_, list) = h.get("/api/v1/boards").await;
    assert!(list["boards"][0]["skills_fit"].is_null(), "{list}");
}
