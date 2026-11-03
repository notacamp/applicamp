//! The watchers over a real Postgres, a fake company site and job board, and
//! a fake model.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use applicamp_ai::BudgetConfig;
use applicamp_kernel::jobs;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{TimeDelta, TimeZone as _, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use super::{
    detect::{self, BoardKind, Outcome, Start, Who, Why},
    store,
};
use crate::{
    SourcesConfig,
    features::search,
    ports::MatchedPosting,
    testkit::{
        Fake, Harness, answer_for, boards_config, completion, gateway, model_scoring, open_facts,
        postings, postings_seen, requests_to, today,
    },
};

fn config_for(site: &MockServer) -> SourcesConfig {
    let mut config = boards_config();
    let url: Url = site.uri().parse().unwrap();
    config.greenhouse_url = url.clone();
    config.lever_url = url.clone();
    config.ashby_url = url;
    config
}

async fn page(server: &MockServer, at: &str, html: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_raw(html.to_owned(), "text/html"))
        .mount(server)
        .await;
}

fn matched(company: &str, site: &str, title: &str, url: &str, days_ago: i64) -> MatchedPosting {
    MatchedPosting {
        opportunity: Uuid::now_v7(),
        company: company.to_owned(),
        site: Some(site.to_owned()),
        title: title.to_owned(),
        url: url.to_owned(),
        apply_url: None,
        match_score: 84,
        shortlisted: false,
        found_at: today() - TimeDelta::days(days_ago),
        work_mode: Some("remote".into()),
        location: Some("Remote - Europe".into()),
        region_note: None,
        salary_text: None,
        description: "Ruby on Rails, remote.".into(),
    }
}

/// Two matches from Northwind Stays, whose site is `site`.
fn two_matches(fake: &Fake, site: &str) {
    let mut found = fake.matched.lock().unwrap();
    found.push(matched(
        "Northwind Stays",
        site,
        "Senior Rails Engineer",
        "https://example.com/jobs/1",
        3,
    ));
    found.push(matched(
        "Northwind Stays",
        site,
        "Backend Engineer",
        "https://example.com/jobs/2",
        10,
    ));
}

async fn post_json(h: &Harness, uri: &str, body: &Value) -> (StatusCode, Value) {
    h.send(
        Request::post(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// A Greenhouse board's list.
fn greenhouse(jobs: &[(u64, &str, &str)]) -> Value {
    json!({ "jobs": jobs.iter().map(|(id, title, place)| json!({
        "id": id,
        "title": title,
        "absolute_url": format!("https://boards.greenhouse.io/northwind/jobs/{id}"),
        "location": { "name": place },
        "company_name": "Northwind Stays",
        "first_published": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
    })).collect::<Vec<_>>() })
}

/// The Greenhouse board `northwind` on `site`: `day_one` on the first read,
/// `day_two` (or `day_one`) on the next ones, with a counter of the reads.
async fn northwind_board(
    site: &MockServer,
    day_one: Value,
    day_two: Option<Value>,
) -> Arc<AtomicUsize> {
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    Mock::given(method("GET"))
        .and(path("/v1/boards/northwind/jobs"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(200).set_body_json(day_one.clone())
            } else {
                ResponseTemplate::new(200)
                    .set_body_json(day_two.clone().unwrap_or_else(|| day_one.clone()))
            }
        })
        .mount(site)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/boards/northwind/jobs/\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "id": 1, "content": "&lt;p&gt;Ruby on Rails, fully remote in Europe.&lt;/p&gt;" }),
        ))
        .mount(site)
        .await;
    reads
}

fn northwind_home() -> &'static str {
    r#"<html><head><title>Northwind Stays</title></head><body>
       <a href="https://boards.greenhouse.io/northwind">Open roles</a></body></html>"#
}

async fn companies(h: &Harness) -> Value {
    let (status, body) = h.get("/api/v1/companies").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn model_calls(model: &MockServer) -> usize {
    model.received_requests().await.unwrap().len()
}

async fn watch_first_suggestion(h: &Harness) -> Uuid {
    let list = companies(h).await;
    let id: Uuid = list["suggested"][0]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (status, body) = h.post(&format!("/api/v1/companies/{id}/watch")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["watching"][0]["finding"], true);
    h.work().await;
    id
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_suggestion_is_watched_its_board_found_and_only_new_jobs_are_scored_next_day(db: PgPool) {
    let site = MockServer::start().await;
    page(&site, "/", northwind_home()).await;
    let reads = northwind_board(
        &site,
        greenhouse(&[
            (4001, "Senior Rails Engineer", "Remote - Europe"),
            (4002, "Office Manager", "Lisbon"),
        ]),
        Some(greenhouse(&[
            (4001, "Senior Rails Engineer", "Remote - Europe"),
            (4002, "Office Manager", "Lisbon"),
            (4003, "Staff Platform Engineer", "Remote - Europe"),
        ])),
    )
    .await;
    let model = model_scoring(82).await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let h = Harness::new(
        db,
        config_for(&site),
        Some(gateway(&model, BudgetConfig::default())),
        fake.clone(),
    )
    .await;

    // The suggestion: two matches in eight weeks, with reasons that cite them.
    let list = companies(&h).await;
    assert_eq!(list["suggested"].as_array().unwrap().len(), 1);
    assert_eq!(list["watching"].as_array().unwrap().len(), 0);
    let first = &list["suggested"][0];
    assert_eq!(first["name"], "Northwind Stays");
    assert_eq!(first["reasons"][0]["kind"], "matches");
    assert_eq!(first["reasons"][0]["count"], 2);
    assert_eq!(first["reasons"][0]["sources"].as_array().unwrap().len(), 2);

    // Watch: the board is found on its site, confirmed, and noted as seen.
    watch_first_suggestion(&h).await;
    let list = companies(&h).await;
    assert_eq!(list["suggested"].as_array().unwrap().len(), 0);
    let watching = &list["watching"][0];
    assert_eq!(watching["board"], "greenhouse");
    assert_eq!(watching["finding"], false);
    assert_eq!(watching["needs_board"], false);
    assert_eq!(watching["open_jobs"], 2);
    assert_eq!(watching["last_error"], Value::Null);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        model_calls(&model).await,
        0,
        "finding a board uses no model"
    );

    // The same day: the board is not read again.
    let (status, _) = h.post("/api/v1/searches").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(model_calls(&model).await, 0);

    // The next day: one new job; only it is scored.
    h.clock.advance(TimeDelta::hours(24));
    let (status, _) = h.post("/api/v1/searches").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert_eq!(model_calls(&model).await, 1);
    let seen = postings_seen(&model).await;
    assert!(seen.contains("Staff Platform Engineer"));
    assert!(
        !seen.contains("Office Manager") && !seen.contains("Senior Rails Engineer"),
        "jobs seen before are not scored"
    );
    let added = fake.added();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].job.title, "Staff Platform Engineer");
    assert_eq!(
        added[0].own_board.as_ref().map(|b| b.company.as_str()),
        Some("Northwind Stays")
    );
    let scans: Vec<(String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT outcome, jobs_read, passed_requirements, scored_by_ai, matched
           FROM sources.board_scans WHERE company_id IS NOT NULL ORDER BY at",
    )
    .fetch_all(&h.db)
    .await
    .unwrap();
    assert_eq!(scans, [("read".to_owned(), 3, 1, 1, 1)]);

    // The day after: nothing new, so no model call.
    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(model_calls(&model).await, 1);
    assert_eq!(fake.added().len(), 1);
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.watch_seen")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(seen, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_hard_requirements_filter_before_any_scoring(db: PgPool) {
    let site = MockServer::start().await;
    let at = Utc
        .with_ymd_and_hms(2026, 10, 24, 10, 0, 0)
        .unwrap()
        .timestamp_millis();
    let posting = move |id: &str, title: &str, mode: &str| {
        json!({
            "id": id, "text": title, "hostedUrl": format!("https://jobs.lever.co/acme/{id}"),
            "createdAt": at, "categories": { "location": "Lisbon" }, "workplaceType": mode,
            "descriptionPlain": "Ruby on Rails.",
        })
    };
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    Mock::given(method("GET"))
        .and(path("/v0/postings/acme"))
        .respond_with(move |_: &wiremock::Request| {
            let list = if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                vec![posting("aaaa0001", "Existing Engineer", "remote")]
            } else {
                vec![
                    posting("aaaa0001", "Existing Engineer", "remote"),
                    posting("aaaa0002", "Fully Remote Engineer", "remote"),
                    posting("aaaa0003", "Office Based Engineer", "on-site"),
                ]
            };
            ResponseTemplate::new(200).set_body_json(list)
        })
        .mount(&site)
        .await;
    let model = model_scoring(82).await;
    let fake = Fake::ready();
    {
        // A posting that came from the company's Lever board says which board.
        let mut found = fake.matched.lock().unwrap();
        for (n, title) in ["Senior Rails Engineer", "Backend Engineer"]
            .iter()
            .enumerate()
        {
            found.push(matched(
                "Acme",
                &format!("{}/", site.uri()),
                title,
                &format!("https://jobs.lever.co/acme/bbbb000{n}"),
                3 + i64::try_from(n).unwrap(),
            ));
        }
    }
    let h = Harness::new(
        db,
        config_for(&site),
        Some(gateway(&model, BudgetConfig::default())),
        fake.clone(),
    )
    .await;
    watch_first_suggestion(&h).await;
    let list = companies(&h).await;
    assert_eq!(list["watching"][0]["board"], "lever");

    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    let seen = postings_seen(&model).await;
    assert!(seen.contains("Fully Remote Engineer"));
    assert!(
        !seen.contains("Office Based Engineer"),
        "an on-site job never reaches the model for someone who works remote only"
    );
    let titles: Vec<String> = fake.added().iter().map(|s| s.job.title.clone()).collect();
    assert_eq!(titles, ["Fully Remote Engineer"]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn not_interested_is_for_good(db: PgPool) {
    let site = MockServer::start().await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let h = Harness::new(db, config_for(&site), None, fake).await;
    let list = companies(&h).await;
    let id = list["suggested"][0]["id"].as_str().unwrap().to_owned();

    let (status, body) = h
        .post(&format!("/api/v1/companies/{id}/not-interested"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["suggested"].as_array().unwrap().len(), 0);

    // Worked out again later, from the same matches: still hidden.
    h.clock.advance(TimeDelta::hours(3));
    assert_eq!(
        companies(&h).await["suggested"].as_array().unwrap().len(),
        0
    );
    h.clock.advance(TimeDelta::days(20));
    assert_eq!(
        companies(&h).await["suggested"].as_array().unwrap().len(),
        0
    );
    // It cannot be watched from the old suggestion either.
    let (status, _) = h.post(&format!("/api/v1/companies/{id}/watch")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_21st_company_is_refused_with_a_clear_message(db: PgPool) {
    let site = MockServer::start().await;
    let h = Harness::new(db, config_for(&site), None, Fake::ready()).await;
    let now = h.clock_now();
    for n in 0..20 {
        let mut conn = h.db.acquire().await.unwrap();
        let key = format!("site:company{n}.test");
        let company = store::ensure_company(
            &mut conn,
            &store::NewCompany {
                key: &key,
                name: &format!("Company {n}"),
                website: Some(&format!("https://company{n}.test/")),
                careers_url: None,
                origin: "matches",
                hint: None,
            },
            now,
        )
        .await
        .unwrap();
        store::start_watching(&mut conn, h.account, company, now)
            .await
            .unwrap();
    }
    let (status, body) = post_json(
        &h,
        "/api/v1/companies",
        &json!({ "url": "https://one-more.test/careers" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "watch_limit_reached");
    assert!(
        body["detail"].as_str().unwrap().contains("20 companies"),
        "{body}"
    );
    // Stopping one makes room.
    let list = companies(&h).await;
    assert_eq!(list["watching"].as_array().unwrap().len(), 20);
    let first = list["watching"][0]["id"].as_str().unwrap().to_owned();
    let (status, _) = h.post(&format!("/api/v1/companies/{first}/stop")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post_json(
        &h,
        "/api/v1/companies",
        &json!({ "url": "https://one-more.test/careers" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_that_fails_keeps_its_watch_and_is_not_asked_again_that_day(db: PgPool) {
    let site = MockServer::start().await;
    page(&site, "/", northwind_home()).await;
    // The second read fails.
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    Mock::given(method("GET"))
        .and(path("/v1/boards/northwind/jobs"))
        .respond_with(move |_: &wiremock::Request| {
            if counter.fetch_add(1, Ordering::SeqCst) == 1 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(greenhouse(&[(
                    4001,
                    "Senior Rails Engineer",
                    "Remote - Europe",
                )]))
            }
        })
        .mount(&site)
        .await;
    let model = model_scoring(82).await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let h = Harness::new(
        db,
        config_for(&site),
        Some(gateway(&model, BudgetConfig::default())),
        fake.clone(),
    )
    .await;
    watch_first_suggestion(&h).await;

    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    let watching = &companies(&h).await["watching"][0];
    assert_eq!(watching["board"], "greenhouse", "the watch is kept");
    assert_eq!(watching["last_error"], "rate_limited");
    let scan: (String, Option<String>) = sqlx::query_as(
        "SELECT outcome, error_kind FROM sources.board_scans WHERE company_id IS NOT NULL",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(scan, ("error".to_owned(), Some("rate_limited".to_owned())));
    assert_eq!(model_calls(&model).await, 0);

    // Not asked again the same day (no retry), and fine again the next.
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    let watching = &companies(&h).await["watching"][0];
    assert_eq!(watching["last_error"], Value::Null);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_new_job_the_model_could_not_score_waits_for_the_next_day(db: PgPool) {
    let site = MockServer::start().await;
    page(&site, "/", northwind_home()).await;
    northwind_board(
        &site,
        greenhouse(&[(4001, "Senior Rails Engineer", "Remote - Europe")]),
        Some(greenhouse(&[
            (4001, "Senior Rails Engineer", "Remote - Europe"),
            (4003, "Staff Platform Engineer", "Remote - Europe"),
        ])),
    )
    .await;
    // The model fails once, then answers.
    let model = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&model)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(|request: &wiremock::Request| {
            let jobs: Vec<Value> = postings(request)
                .iter()
                .map(|posting| answer_for(posting["ref"].as_str().unwrap_or(""), 82, &open_facts()))
                .collect();
            ResponseTemplate::new(200).set_body_json(completion(&json!({ "jobs": jobs }), 0.000_4))
        })
        .mount(&model)
        .await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let h = Harness::new(
        db,
        config_for(&site),
        Some(gateway(&model, BudgetConfig::default())),
        fake.clone(),
    )
    .await;
    watch_first_suggestion(&h).await;

    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert!(fake.added().is_empty(), "the model failed");
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.watch_seen")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(seen, 1, "the job it could not score is not noted as seen");

    h.clock.advance(TimeDelta::hours(24));
    h.post("/api/v1/searches").await;
    h.work().await;
    assert_eq!(fake.added().len(), 1);
    assert_eq!(fake.added()[0].job.title, "Staff Platform Engineer");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_nightly_job_recurs_and_starts_a_search_for_a_due_watch(db: PgPool) {
    let site = MockServer::start().await;
    page(&site, "/", northwind_home()).await;
    northwind_board(
        &site,
        greenhouse(&[(4001, "Senior Rails Engineer", "Remote - Europe")]),
        None,
    )
    .await;
    let model = model_scoring(82).await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let mut config = config_for(&site);
    config.nightly = Some(true);
    let h = Harness::new(
        db,
        config,
        Some(gateway(&model, BudgetConfig::default())),
        fake.clone(),
    )
    .await;
    fake.accounts.lock().unwrap().push(h.account);
    watch_first_suggestion(&h).await;
    h.clock.advance(TimeDelta::hours(24));

    // Tonight's nightly job runs once.
    sqlx::query("DELETE FROM kernel.jobs WHERE kind = 'sources.nightly'")
        .execute(&h.db)
        .await
        .unwrap();
    let mut conn = h.db.acquire().await.unwrap();
    jobs::enqueue(&mut conn, &search::Nightly {}).await.unwrap();
    drop(conn);
    h.work_once().await;

    // It scheduled tomorrow's (a running job does not count itself) ...
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM kernel.jobs WHERE kind = 'sources.nightly' AND state = 'pending'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(pending, 1, "the nightly job recurs");
    // ... and, with no new job on any job board, started a search because a
    // watched board is due.
    let searches: Vec<(String, String)> =
        sqlx::query_as("SELECT trigger, status FROM sources.searches")
            .fetch_all(&h.db)
            .await
            .unwrap();
    assert_eq!(searches, [("nightly".to_owned(), "queued".to_owned())]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn no_board_is_found_until_a_careers_page_that_has_one_is_pasted(db: PgPool) {
    let site = MockServer::start().await;
    page(&site, "/", "<html><body>Invented software.</body></html>").await;
    northwind_board(
        &site,
        greenhouse(&[(4001, "Senior Rails Engineer", "Remote - Europe")]),
        None,
    )
    .await;
    page(
        &site,
        "/work-here",
        r#"<html><body><a href="https://boards.greenhouse.io/northwind">Jobs</a></body></html>"#,
    )
    .await;
    let fake = Fake::ready();
    two_matches(&fake, &format!("{}/", site.uri()));
    let h = Harness::new(db, config_for(&site), None, fake).await;
    let id = watch_first_suggestion(&h).await;
    let watching = &companies(&h).await["watching"][0];
    assert_eq!(watching["board"], "none");
    assert_eq!(watching["needs_board"], true);
    assert_eq!(watching["last_error"], "no_board");

    // A page that does not parse as a link.
    let (status, body) = post_json(
        &h,
        &format!("/api/v1/companies/{id}/careers"),
        &json!({ "url": "   " }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalid_url");

    let (status, body) = post_json(
        &h,
        &format!("/api/v1/companies/{id}/careers"),
        &json!({ "url": format!("{}/work-here", site.uri()) }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["watching"][0]["finding"], true);
    h.work().await;
    let watching = &companies(&h).await["watching"][0];
    assert_eq!(watching["board"], "greenhouse");
    assert_eq!(watching["needs_board"], false);
    assert_eq!(watching["open_jobs"], 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_company_can_be_watched_by_pasting_its_careers_page(db: PgPool) {
    let site = MockServer::start().await;
    page(
        &site,
        "/careers",
        r#"<html><head><meta property="og:site_name" content="Contoso Health"></head>
           <body><a href="https://boards.greenhouse.io/northwind">Jobs</a></body></html>"#,
    )
    .await;
    northwind_board(
        &site,
        greenhouse(&[(4001, "Senior Rails Engineer", "Remote - Europe")]),
        None,
    )
    .await;
    let h = Harness::new(db, config_for(&site), None, Fake::ready()).await;
    let (status, body) = post_json(
        &h,
        "/api/v1/companies",
        &json!({ "url": format!("{}/careers", site.uri()) }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["watching"].as_array().unwrap().len(), 1);
    h.work().await;
    let watching = &companies(&h).await["watching"][0];
    assert_eq!(
        watching["name"], "Contoso Health",
        "the page names the company"
    );
    assert_eq!(watching["board"], "greenhouse");
    let origin: String =
        sqlx::query_scalar("SELECT origin FROM sources.companies WHERE name = 'Contoso Health'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(origin, "pasted");
    // LinkedIn is not a company.
    let (status, _) = post_json(
        &h,
        "/api/v1/companies",
        &json!({ "url": "https://www.linkedin.com/company/contoso/jobs" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn who() -> Who {
    Who {
        id: Uuid::now_v7(),
        name: "Acme".into(),
    }
}

async fn found(site: &MockServer, deps: &crate::SourcesDeps, db: &PgPool) -> Outcome {
    let origin: Url = format!("{}/", site.uri()).parse().unwrap();
    detect::find(deps, db, &who(), Start::Site(origin)).await
}

fn deps_for(site: &MockServer) -> crate::SourcesDeps {
    crate::SourcesDeps::new(config_for(site), None, Fake::ready()).unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn every_board_kind_is_detected_on_a_companys_own_site(db: PgPool) {
    // Greenhouse, from a link on the home page.
    let site = MockServer::start().await;
    page(&site, "/", northwind_home()).await;
    northwind_board(
        &site,
        greenhouse(&[(4001, "Senior Rails Engineer", "Remote - Europe")]),
        None,
    )
    .await;
    let Outcome::Found(d) = found(&site, &deps_for(&site), &db).await else {
        panic!("a Greenhouse board");
    };
    assert_eq!(
        (d.kind, d.token.as_deref(), d.jobs.len()),
        (BoardKind::Greenhouse, Some("northwind"), 1)
    );

    // Lever, on a careers page the home page links to.
    let site = MockServer::start().await;
    page(
        &site,
        "/",
        r#"<a href="/company/careers">We are hiring</a>"#,
    )
    .await;
    page(
        &site,
        "/company/careers",
        r#"<a href="https://jobs.lever.co/acme">Open roles</a>"#,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/v0/postings/acme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": "AAAA0001", "text": "Engineer", "hostedUrl": "https://jobs.lever.co/acme/aaaa0001",
            "categories": { "location": "Remote" }, "workplaceType": "remote",
        }])))
        .mount(&site)
        .await;
    let Outcome::Found(d) = found(&site, &deps_for(&site), &db).await else {
        panic!("a Lever board");
    };
    assert_eq!(
        (d.kind, d.token.as_deref()),
        (BoardKind::Lever, Some("acme"))
    );
    assert!(d.careers_url.unwrap().path().ends_with("/company/careers"));

    // Ashby, on /jobs, which no page links to.
    let site = MockServer::start().await;
    page(&site, "/", "<html>Invented.</html>").await;
    page(
        &site,
        "/jobs",
        r#"<iframe src="https://jobs.ashbyhq.com/acme"></iframe>"#,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/posting-api/job-board/acme"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "jobs": [{
            "id": "bbbb0001", "title": "Engineer", "location": "Remote", "isRemote": true,
            "jobUrl": "https://jobs.ashbyhq.com/acme/bbbb0001",
        }] })))
        .mount(&site)
        .await;
    let Outcome::Found(d) = found(&site, &deps_for(&site), &db).await else {
        panic!("an Ashby board");
    };
    assert_eq!(
        (d.kind, d.token.as_deref(), d.jobs.len()),
        (BoardKind::Ashby, Some("acme"), 1)
    );

    // schema.org JobPosting data on /careers.
    let site = MockServer::start().await;
    page(&site, "/", "<html>Invented.</html>").await;
    page(
        &site,
        "/careers",
        r#"<html><script type="application/ld+json">
           {"@context":"https://schema.org","@graph":[
             {"@type":"JobPosting","title":"Platform Engineer","url":"https://acme.test/careers/1",
              "hiringOrganization":{"name":"Acme"},"datePosted":"2026-10-24","description":"<p>Invented.</p>"},
             {"@type":"JobPosting","title":"Designer","url":"https://acme.test/careers/2",
              "hiringOrganization":{"name":"Acme"},"datePosted":"2026-10-24"}]}
           </script></html>"#,
    )
    .await;
    let Outcome::Found(d) = found(&site, &deps_for(&site), &db).await else {
        panic!("JobPosting data");
    };
    assert_eq!(
        (d.kind, d.token, d.jobs.len()),
        (BoardKind::Jsonld, None, 2)
    );
    assert!(d.careers_url.unwrap().path().ends_with("/careers"));
    assert_eq!(d.jobs[0].source_name(), "careers");

    // Nothing.
    let site = MockServer::start().await;
    page(&site, "/", "<html>Invented software, no jobs.</html>").await;
    page(&site, "/careers", "<html>Check back later.</html>").await;
    assert!(matches!(
        found(&site, &deps_for(&site), &db).await,
        Outcome::Nothing(Why::NoBoard)
    ));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn robots_txt_is_respected_while_looking_for_a_board(db: PgPool) {
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "User-agent: *\nDisallow: /careers\nDisallow: /jobs\n",
            "text/plain",
        ))
        .mount(&site)
        .await;
    page(&site, "/", "<html>Invented.</html>").await;
    page(
        &site,
        "/careers",
        r#"<a href="https://boards.greenhouse.io/northwind">x</a>"#,
    )
    .await;
    northwind_board(&site, greenhouse(&[(4001, "Engineer", "Remote")]), None).await;
    assert!(matches!(
        found(&site, &deps_for(&site), &db).await,
        Outcome::Nothing(Why::NoBoard)
    ));
    assert_eq!(
        requests_to(&site, "/careers").await,
        0,
        "a forbidden page is never read"
    );
    assert_eq!(requests_to(&site, "/jobs").await, 0);

    // A site that forbids everything is not read at all.
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("User-agent: *\nDisallow: /\n", "text/plain"),
        )
        .mount(&site)
        .await;
    page(&site, "/", northwind_home()).await;
    assert!(matches!(
        found(&site, &deps_for(&site), &db).await,
        Outcome::Nothing(Why::RobotsDisallows)
    ));
    assert_eq!(
        requests_to(&site, "/").await,
        1,
        "only robots.txt was asked"
    );
}
