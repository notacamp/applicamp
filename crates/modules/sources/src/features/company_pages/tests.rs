use std::sync::Arc;

use axum::{body::Body, http::Request};
use chrono::{TimeZone as _, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use super::*;
use crate::{
    SearchConfig, SourcesConfig, company_pages as registry,
    hiring_systems::CompanyLink,
    testkit::{Fake, Harness, boards_config, requests_to, today},
};

const WORKABLE: &str = include_str!("../../readers/fixtures/workable.json");
const RECRUITEE: &str = include_str!("../../readers/fixtures/recruitee.json");
const TEAMTAILOR: &str = include_str!("../../readers/fixtures/teamtailor.rss");
const PERSONIO: &str = include_str!("../../readers/fixtures/personio.xml");

/// A hiring system's fake: robots rules (with a content signal line, when
/// one is given) and a home page with no terms.
async fn site(signal: Option<&str>) -> MockServer {
    let server = MockServer::start().await;
    let robots = match signal {
        Some(line) => format!("User-agent: *\n{line}\nDisallow: /private\n"),
        None => "User-agent: *\nDisallow: /private\n".to_owned(),
    };
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(robots.into_bytes(), "text/plain"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"<html lang=\"en\"><body>An invented hiring system.</body></html>".to_vec(),
            "text/html",
        ))
        .mount(&server)
        .await;
    server
}

/// A Greenhouse list: one job per link, all published at `posted`.
fn greenhouse_list(posted: &str, links: &[&str]) -> String {
    let jobs: Vec<_> = links
        .iter()
        .enumerate()
        .map(|(i, link)| {
            json!({
                "id": 1000 + i,
                "title": format!("Engineer {i}"),
                "absolute_url": link,
                "location": { "name": "Remote" },
                "first_published": posted,
            })
        })
        .collect();
    json!({ "jobs": jobs }).to_string()
}

async fn mount_greenhouse(server: &MockServer, token: &str, body: String) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/boards/{token}/jobs")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json"),
        )
        .mount(server)
        .await;
}

/// Settings reading Greenhouse at `greenhouse`, Lever at `lever` and every
/// other system at `others`, with the daily jobs off.
fn config(greenhouse: &MockServer, lever: &MockServer, others: &MockServer) -> SourcesConfig {
    let url = |server: &MockServer| server.uri().parse().unwrap();
    SourcesConfig {
        greenhouse_url: url(greenhouse),
        lever_url: url(lever),
        lever_eu_url: url(lever),
        ashby_url: url(others),
        workable_url: url(others),
        recruitee_url: others.uri(),
        teamtailor_url: others.uri(),
        personio_url: others.uri(),
        company_pages: Some(false),
        ..boards_config()
    }
}

fn link(source: Source, token: &str) -> CompanyLink {
    CompanyLink {
        source,
        token: token.to_owned(),
        eu: false,
    }
}

async fn row_of(db: &PgPool, system: &str, token: Option<&str>) -> BoardRow {
    let all = job_boards::all(db).await.unwrap();
    all.into_iter()
        .find(|row| {
            row.system.as_deref() == Some(system)
                && row.company_token.as_deref() == token
                && row.kind().is_some_and(Kind::is_hiring)
        })
        .unwrap_or_else(|| panic!("no row for {system} {token:?}"))
}

fn owner_harness_parts() -> Arc<Fake> {
    let fake = Fake::ready();
    fake.signed_in_as("owner@example.com");
    fake
}

fn owners(config: SourcesConfig) -> SourcesConfig {
    SourcesConfig {
        board_owners: Some(vec!["owner@example.com".into()]),
        ..config
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_clean_system_and_its_company_page_are_approved_by_the_app(db: PgPool) {
    let greenhouse = site(None).await;
    mount_greenhouse(
        &greenhouse,
        "acme",
        greenhouse_list(
            "2026-10-20T10:00:00Z",
            &["https://boards.greenhouse.io/acme/jobs/1"],
        ),
    )
    .await;
    let (lever, others) = (site(None).await, site(None).await);
    let h = Harness::new(
        db,
        config(&greenhouse, &lever, &others),
        None,
        Fake::ready(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    let added = registry::register(
        &h.db,
        &[link(Source::Greenhouse, "acme")],
        "an apply link on Fake Board",
        today(),
    )
    .await
    .unwrap();
    assert_eq!(added, 1);
    // The same company again, in other letters: still one page.
    assert_eq!(
        registry::register(
            &h.db,
            &[link(Source::Greenhouse, "ACME")],
            "web search",
            today()
        )
        .await
        .unwrap(),
        0
    );

    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(
        summary,
        Summary {
            systems: 7,
            pages: 1,
            approved: 1
        }
    );
    let system = row_of(&h.db, "greenhouse", None).await;
    assert_eq!(system.status(), Status::Approved);
    assert_eq!(system.approved_by.as_deref(), Some("app"));
    assert!(system.approved_at.is_some());
    assert_eq!(system.terms_verdict.as_deref(), Some("silent"));
    let page = row_of(&h.db, "greenhouse", Some("acme")).await;
    assert_eq!(page.status(), Status::Approved);
    assert_eq!(page.approved_by.as_deref(), Some("app"));
    assert_eq!(page.name, "Acme");
    assert_eq!(page.system_id, Some(system.id));
    assert_eq!(
        page.found_through.as_deref(),
        Some("an apply link on Fake Board")
    );
    assert_eq!(page.last_fetch_jobs, Some(1));
    assert_eq!(requests_to(&greenhouse, "/robots.txt").await, 1);
    assert_eq!(requests_to(&greenhouse, "/v1/boards/acme/jobs").await, 1);

    // The list says what each is.
    let (_, list) = h.get("/api/v1/boards").await;
    let boards = list["boards"].as_array().unwrap();
    let shown = boards
        .iter()
        .find(|b| b["kind"] == "company_page")
        .expect("the company page is listed");
    assert_eq!(shown["system"], "greenhouse");
    assert_eq!(shown["company_token"], "acme");
    assert_eq!(shown["approved_by"], "app");
    assert_eq!(
        boards
            .iter()
            .filter(|b| b["kind"] == "hiring_system")
            .count(),
        7
    );
    assert_eq!(list["company_pages"]["total"], 1);
    assert_eq!(list["company_pages"]["approved"], 1);
    assert_eq!(list["web_search"]["on"], false);

    // Nothing is asked again within the day, and no job store is read by a
    // search: only the approved job boards are.
    let again = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(again, Summary::default());
    assert!(job_boards::approved(&h.db).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_system_that_asks_ai_tools_not_to_learn_waits_for_the_owner_and_so_does_its_page(
    db: PgPool,
) {
    let greenhouse = site(None).await;
    let lever = site(Some("Content-Signal: search=yes, ai-train=no")).await;
    let others = site(None).await;
    let h = Harness::new(
        db,
        owners(config(&greenhouse, &lever, &others)),
        None,
        owner_harness_parts(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    registry::register(
        &h.db,
        &[link(Source::Lever, "northlight")],
        "an apply link on Fake Board",
        today(),
    )
    .await
    .unwrap();
    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(summary.approved, 0);
    assert_eq!(summary.pages, 0, "a page whose system waits is not read");
    assert_eq!(requests_to(&lever, "/v0/postings").await, 0);

    let system = row_of(&h.db, "lever", None).await;
    assert_eq!(
        system.status(),
        Status::Candidate,
        "never approved over the signal"
    );
    let page = row_of(&h.db, "lever", Some("northlight")).await;
    assert_eq!(page.status(), Status::Candidate);
    // The owner sees why, and approving is the choice.
    let (_, detail) = h.get(&format!("/api/v1/boards/{}", system.id)).await;
    assert_eq!(detail["blockers"][0]["code"], "needs_choice", "{detail}");
    assert_eq!(detail["blocked"], serde_json::Value::Null);
    assert_eq!(detail["approvable"], true);
    assert_eq!(detail["board"]["content_signal"]["ai-train"], "no");
    // Its page cannot be approved before its system.
    let (status, problem) = h.post(&format!("/api/v1/boards/{}/approve", page.id)).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "system_waiting", "{problem}");

    let (status, approved) = h
        .post(&format!("/api/v1/boards/{}/approve", system.id))
        .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{approved}");
    assert_eq!(approved["board"]["approved_by"], "owner");
    Mock::given(method("GET"))
        .and(path("/v0/postings/northlight"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!([{
                    "id": "0b6d1c2e-4f1c-4a55-8a1d-2f1c0b6d1c2e",
                    "text": "Engineer",
                    "hostedUrl": "https://jobs.lever.co/northlight/0b6d1c2e",
                    "createdAt": 1_792_454_400_000_i64,
                    "categories": { "location": "Remote" },
                    "workplaceType": "remote",
                    "descriptionPlain": "Invented.",
                    "lists": []
                }])
                .to_string()
                .into_bytes(),
                "application/json",
            ),
        )
        .mount(&lever)
        .await;
    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!((summary.pages, summary.approved), (1, 1));
    let page = row_of(&h.db, "lever", Some("northlight")).await;
    assert_eq!(page.status(), Status::Approved);
    assert_eq!(page.approved_by.as_deref(), Some("app"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_company_page_with_no_recent_job_waits_and_is_not_asked_again_for_a_month(db: PgPool) {
    let greenhouse = site(None).await;
    mount_greenhouse(
        &greenhouse,
        "oldco",
        greenhouse_list(
            "2026-01-05T10:00:00Z",
            &["https://boards.greenhouse.io/oldco/jobs/1"],
        ),
    )
    .await;
    let (lever, others) = (site(None).await, site(None).await);
    let h = Harness::new(
        db,
        config(&greenhouse, &lever, &others),
        None,
        Fake::ready(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    registry::register(
        &h.db,
        &[link(Source::Greenhouse, "oldco")],
        "web search",
        today(),
    )
    .await
    .unwrap();
    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!((summary.pages, summary.approved), (1, 0));
    let page = row_of(&h.db, "greenhouse", Some("oldco")).await;
    assert_eq!(page.status(), Status::Candidate);
    let report = page.report().unwrap();
    assert!(
        report
            .flags
            .iter()
            .any(|f| f == "no job was posted in the last 60 days"),
        "{:?}",
        report.flags
    );
    assert_eq!(page.recheck_after, Some(today() + PAGE_RETRY));
    // A month on, it is read again.
    let later = today() + PAGE_RETRY + TimeDelta::days(1);
    h.clock.set(later);
    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(summary.pages, 1);
    assert_eq!(requests_to(&greenhouse, "/v1/boards/oldco/jobs").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn declining_a_system_declines_its_pages_and_the_pages_found_after(db: PgPool) {
    let (greenhouse, lever, others) = (site(None).await, site(None).await, site(None).await);
    let h = Harness::new(
        db,
        owners(config(&greenhouse, &lever, &others)),
        None,
        owner_harness_parts(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    registry::register(
        &h.db,
        &[link(Source::Ashby, "first")],
        "web search",
        today(),
    )
    .await
    .unwrap();
    let system = row_of(&h.db, "ashby", None).await;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/boards/{}/decline", system.id))
        .header("content-type", "application/json")
        .body(Body::from(json!({ "reason": "Not wanted." }).to_string()))
        .unwrap();
    let (status, _) = h.send(request).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let first = row_of(&h.db, "ashby", Some("first")).await;
    assert_eq!(first.status(), Status::Declined);
    assert_eq!(
        first.decline_reason.as_deref(),
        Some("Its hiring system was declined.")
    );
    registry::register(
        &h.db,
        &[link(Source::Ashby, "second")],
        "web search",
        today(),
    )
    .await
    .unwrap();
    assert_eq!(
        row_of(&h.db, "ashby", Some("second")).await.status(),
        Status::Declined,
        "declined on arrival"
    );
    // A declined system is never checked, and its pages never read.
    let summary = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(summary.systems, 6);
    assert_eq!(summary.pages, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_apply_links_of_jobs_read_register_each_company_page_once(db: PgPool) {
    let greenhouse = site(None).await;
    mount_greenhouse(
        &greenhouse,
        "acme",
        greenhouse_list(
            "2026-10-20T10:00:00Z",
            &[
                "https://jobs.lever.co/acme-lever/0b6d",
                "https://boards.greenhouse.io/acme/jobs/9",
                "https://example.com/apply/1",
            ],
        ),
    )
    .await;
    let (lever, others) = (site(None).await, site(None).await);
    let h = Harness::new(
        db,
        config(&greenhouse, &lever, &others),
        None,
        Fake::ready(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    let reader = h.deps.reader.as_ref().unwrap();
    let board = Board {
        kind: Source::Greenhouse,
        token: "acme".into(),
        company: "Acme".into(),
        eu: false,
    };
    assert_eq!(reader.board(&h.db, &board).await.unwrap().len(), 3);
    assert_eq!(reader.board(&h.db, &board).await.unwrap().len(), 3);
    let pages: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT system, company_token, found_through FROM sources.job_boards
          WHERE kind = 'company_page' ORDER BY system",
    )
    .fetch_all(&h.db)
    .await
    .unwrap();
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert_eq!(pages[0].0, "greenhouse");
    assert_eq!(pages[1].1, "acme-lever");
    assert_eq!(pages[1].2.as_deref(), Some("an apply link on Acme careers"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_day_checks_no_more_company_pages_than_its_limit(db: PgPool) {
    let greenhouse = site(None).await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/boards/[\w-]+/jobs$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            greenhouse_list("2026-10-24T10:00:00Z", &["https://example.com/apply/1"]).into_bytes(),
            "application/json",
        ))
        .mount(&greenhouse)
        .await;
    let (lever, others) = (site(None).await, site(None).await);
    let h = Harness::new(
        db,
        SourcesConfig {
            company_page_checks: Some(2),
            ..config(&greenhouse, &lever, &others)
        },
        None,
        Fake::ready(),
    )
    .await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    registry::register(
        &h.db,
        &[
            link(Source::Greenhouse, "one"),
            link(Source::Greenhouse, "two"),
            link(Source::Greenhouse, "three"),
        ],
        "web search",
        today(),
    )
    .await
    .unwrap();
    let first = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!((first.pages, first.approved), (2, 2));
    let second = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(second.pages, 0, "the day's limit is spent");
    assert_eq!(requests_to(&greenhouse, "/v1/boards").await, 2);
    // Tomorrow takes the third.
    h.clock.set(today() + TimeDelta::days(1));
    let third = run_all(&h.deps, &h.db).await.unwrap();
    assert_eq!(third.pages, 1);
    assert_eq!(requests_to(&greenhouse, "/v1/boards").await, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn each_new_system_is_read_at_its_own_address(db: PgPool) {
    let server = MockServer::start().await;
    let serve = |p: &'static str, body: &'static str, kind: &'static str| {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), kind))
    };
    serve("/api/v1/widget/accounts/acme", WORKABLE, "application/json")
        .mount(&server)
        .await;
    serve("/api/offers", RECRUITEE, "application/json")
        .mount(&server)
        .await;
    serve("/jobs.rss", TEAMTAILOR, "application/rss+xml")
        .mount(&server)
        .await;
    serve("/xml", PERSONIO, "text/xml").mount(&server).await;
    let config = SourcesConfig {
        workable_url: server.uri().parse().unwrap(),
        recruitee_url: server.uri(),
        teamtailor_url: server.uri(),
        personio_url: server.uri(),
        ..boards_config()
    };
    let reader = Reader::new(Arc::new(config)).unwrap();
    let read = |kind: Source| {
        let board = Board {
            kind,
            token: "acme".into(),
            company: "Acme".into(),
            eu: false,
        };
        let reader = reader.clone();
        let db = db.clone();
        async move { reader.board(&db, &board).await.unwrap() }
    };
    assert_eq!(read(Source::Workable).await.len(), 3);
    assert_eq!(read(Source::Recruitee).await.len(), 2);
    assert_eq!(read(Source::Teamtailor).await.len(), 2);
    assert_eq!(read(Source::Personio).await.len(), 2);
    // A token that is not a name for a host is never asked.
    let bad = Board {
        kind: Source::Recruitee,
        token: "a.b".into(),
        company: "Acme".into(),
        eu: false,
    };
    assert!(reader.board(&db, &bad).await.is_err());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_watched_company_is_linked_to_its_page(db: PgPool) {
    registry::ensure_systems(&db, today()).await.unwrap();
    let company = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sources.companies (id, key, name, origin, created_at)
         VALUES ($1, 'ats:greenhouse:acme', 'Acme', 'pasted', $2)",
    )
    .bind(company)
    .bind(today())
    .execute(&db)
    .await
    .unwrap();
    let page = registry::link_company(&db, company, &link(Source::Greenhouse, "acme"), today())
        .await
        .unwrap()
        .expect("the page is there");
    let linked: Option<Uuid> =
        sqlx::query_scalar("SELECT page_id FROM sources.companies WHERE id = $1")
            .bind(company)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(linked, Some(page));
    let row = job_boards::get(&db, page).await.unwrap().unwrap();
    assert_eq!(row.found_through.as_deref(), Some("your company watch"));
    // Watching it again links the same page.
    assert_eq!(
        registry::link_company(&db, company, &link(Source::Greenhouse, "Acme"), today())
            .await
            .unwrap(),
        Some(page)
    );
}

#[test]
fn the_web_search_asks_every_role_of_every_system_in_turn() {
    let systems: Vec<&System> = hiring_systems::SYSTEMS.iter().take(2).collect();
    let roles = vec![
        "Senior \"Backend\" Engineer".to_owned(),
        "senior backend engineer".to_owned(),
        "UX".to_owned(),
        "Data Scientist".to_owned(),
    ];
    let all = web_search::queries(&roles, &systems);
    assert_eq!(
        all,
        [
            "site:boards.greenhouse.io \"data scientist\"",
            "site:jobs.lever.co \"data scientist\"",
            "site:boards.greenhouse.io \"senior backend engineer\"",
            "site:jobs.lever.co \"senior backend engineer\"",
        ],
        "plain, lower case, once each; a role of two letters is no query"
    );
    let week = |n: i64| Utc.timestamp_opt(n * 7 * 24 * 3600, 0).unwrap();
    let first = web_search::this_weeks(&all, 3, week(10));
    let next = web_search::this_weeks(&all, 3, week(11));
    assert_eq!(first.len(), 3);
    assert_ne!(first, next, "the window moves along the list");
    assert!(web_search::this_weeks(&all, 0, week(1)).is_empty());
    assert!(web_search::this_weeks(&[], 5, week(1)).is_empty());
    let monday = Utc.with_ymd_and_hms(2026, 11, 2, 5, 30, 0).unwrap();
    assert_eq!(
        web_search::next_run(Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()),
        monday
    );
    assert_eq!(
        web_search::month_start(Utc.with_ymd_and_hms(2026, 11, 20, 9, 0, 0).unwrap()),
        Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn only_the_links_of_a_search_answer_are_kept() {
    let answer = json!({
        "query": { "original": "never read" },
        "web": { "results": [
            { "url": "https://jobs.lever.co/zeta/1", "title": "A title", "description": "A snippet" },
            { "url": "https://example.com/careers", "title": "Not a system" },
            { "url": "https://zeta.recruitee.com/o/x" },
            { "url": "https://jobs.lever.co/zeta/2" }
        ] }
    });
    let links = web_search::links_of(&answer);
    assert_eq!(
        links,
        [link(Source::Lever, "zeta"), link(Source::Recruitee, "zeta")]
    );
    assert!(web_search::links_of(&json!({})).is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_web_search_is_counted_capped_and_keeps_links_only(db: PgPool) {
    let brave = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/res/v1/web/search"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({ "web": { "results": [
                { "url": "https://jobs.lever.co/zeta/1", "title": "Never kept" },
                { "url": "https://example.com/x" }
            ] } })
                .to_string()
                .into_bytes(),
                "application/json",
            ),
        )
        .mount(&brave)
        .await;
    let key = std::env::temp_dir().join(format!("applicamp-brave-key-{}", Uuid::now_v7()));
    std::fs::write(&key, "test-key\n").unwrap();
    let (greenhouse, lever, others) = (site(None).await, site(None).await, site(None).await);
    let fake = Fake::ready();
    fake.accounts.lock().unwrap().push(Uuid::now_v7());
    let h = Harness::new(
        db,
        SourcesConfig {
            search: SearchConfig {
                brave_api_key_file: Some(key.clone()),
                brave_url: brave.uri().parse().unwrap(),
                brave_weekly_queries: Some(3),
                brave_monthly_queries: Some(4),
            },
            ..config(&greenhouse, &lever, &others)
        },
        None,
        fake,
    )
    .await;
    // Every system is approved first: only they are searched.
    run_all(&h.deps, &h.db).await.unwrap();

    let first = web_search::run(&h.deps, &h.db).await.unwrap();
    assert_eq!(first.queries, 3);
    assert_eq!(first.added, 1, "the same link twice is one page");
    let asked = brave.received_requests().await.unwrap();
    assert_eq!(asked.len(), 3);
    assert_eq!(
        asked[0]
            .headers
            .get("x-subscription-token")
            .unwrap()
            .to_str()
            .unwrap(),
        "test-key"
    );
    let page = row_of(&h.db, "lever", Some("zeta")).await;
    assert_eq!(page.found_through.as_deref(), Some("web search"));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.web_search_queries")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(rows, 3);

    // The month allows four: one more, then none.
    assert_eq!(web_search::run(&h.deps, &h.db).await.unwrap().queries, 1);
    assert_eq!(web_search::run(&h.deps, &h.db).await.unwrap().queries, 0);
    assert_eq!(queries_this_month(&h.db, today()).await.unwrap(), 4);
    let (_, list) = h.get("/api/v1/boards").await;
    assert_eq!(list["web_search"]["on"], true);
    assert_eq!(list["web_search"]["queries_this_month"], 4);
    assert_eq!(list["web_search"]["monthly_limit"], 4);
    std::fs::remove_file(&key).ok();
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_web_search_is_off_without_a_key(db: PgPool) {
    let (greenhouse, lever, others) = (site(None).await, site(None).await, site(None).await);
    let h = Harness::new(
        db,
        config(&greenhouse, &lever, &others),
        None,
        Fake::ready(),
    )
    .await;
    assert_eq!(
        web_search::run(&h.deps, &h.db).await.unwrap(),
        web_search::Summary::default()
    );
    assert!(!web_search::schedule(&h.db, &h.deps).await.unwrap());
}

#[test]
fn the_daily_check_runs_at_five() {
    let evening = Utc.with_ymd_and_hms(2026, 11, 4, 18, 0, 0).unwrap();
    assert_eq!(
        next_run(evening),
        Utc.with_ymd_and_hms(2026, 11, 5, 5, 0, 0).unwrap()
    );
    let early = Utc.with_ymd_and_hms(2026, 11, 4, 3, 0, 0).unwrap();
    assert_eq!(
        next_run(early),
        Utc.with_ymd_and_hms(2026, 11, 4, 5, 0, 0).unwrap()
    );
}
