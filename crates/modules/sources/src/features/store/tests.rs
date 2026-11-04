use chrono::{DateTime, TimeDelta, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::*;
use crate::{
    SourcesConfig, company_pages as registry,
    hiring_systems::CompanyLink,
    job::{BoardJob, Job, Source, WorkMode},
    testkit::{Fake, Harness, boards_config, requests_to, today},
};

/// A board of the registry, as a seed list writes it: a JSON API at
/// `server`, `/<slug>`, read at `/jobs`, its site `<slug>.example`.
fn board(slug: &str, server: &MockServer, status: &str, config: &str, terms: &str) -> String {
    format!(
        r#"
[[board]]
slug = "{slug}"
name = "Board {slug}"
homepage = "https://{slug}.example"
kind = "json_api"
priority = "high"
status = "{status}"

[board.config]
endpoint = "{uri}/{slug}"
items = "/jobs"
poll_minutes = 15
{config}

[board.config.fields]
ref = "/id"
title = "/title"
company = "/company"
url = "/url"
description = "/description"
posted_at = "/posted"
expires_at = "/expires"

[board.terms]
attribution = "Board {slug}"
{terms}
"#,
        uri = server.uri()
    )
}

async fn seed(db: &PgPool, boards: &[String]) {
    let seed = job_boards::parse(&boards.join("\n")).unwrap();
    job_boards::sync(db, &seed, today()).await.unwrap();
}

/// What a fake board lists: `(id, title)`, all posted on the 25th.
fn listing(jobs: &[(&str, &str)]) -> Value {
    json!({ "jobs": jobs.iter().map(|(id, title)| json!({
        "id": id,
        "title": title,
        "company": "Acme",
        "url": format!("https://example.com/jobs/{id}"),
        "description": "<p>We build things in Rust</p>",
        "posted": "2026-10-25T10:00:00Z",
    })).collect::<Vec<_>>() })
}

async fn serve(server: &MockServer, at: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(body.to_string().into_bytes(), "application/json"),
        )
        .mount(server)
        .await;
}

fn store_config() -> SourcesConfig {
    SourcesConfig {
        store: Some(true),
        ..boards_config()
    }
}

async fn harness(db: PgPool, config: SourcesConfig) -> Harness {
    Harness::new(db, config, None, Fake::ready()).await
}

async fn count(db: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db).await.unwrap()
}

async fn id_of(db: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM sources.job_boards WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .unwrap()
}

/// A line of the log of reads.
#[derive(Debug, sqlx::FromRow)]
struct Line {
    status: String,
    listed: i32,
    new_jobs: i32,
    changed_jobs: i32,
    closed_jobs: i32,
    dropped_jobs: i32,
    http_status: Option<i32>,
    etag: Option<String>,
    error_kind: Option<String>,
    quiet: i32,
    at: DateTime<Utc>,
    next_read_at: DateTime<Utc>,
}

async fn last(db: &PgPool, slug: &str) -> Line {
    sqlx::query_as(
        "SELECT r.status, r.listed, r.new_jobs, r.changed_jobs, r.closed_jobs, r.dropped_jobs,
                r.http_status, r.etag, r.error_kind, r.quiet, r.at, r.next_read_at
           FROM sources.store_reads r JOIN sources.job_boards b ON b.id = r.board_id
          WHERE b.slug = $1 ORDER BY r.at DESC, r.id DESC LIMIT 1",
    )
    .bind(slug)
    .fetch_one(db)
    .await
    .unwrap()
}

async fn open_jobs(db: &PgPool, slug: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM sources.store_jobs j JOIN sources.job_boards b ON b.id = j.board_id
          WHERE b.slug = $1 AND j.closed_at IS NULL",
    )
    .bind(slug)
    .fetch_one(db)
    .await
    .unwrap()
}

fn limits() -> Limits {
    Limits {
        description_chars: 1_000,
        max_per_site: 100,
    }
}

fn job(reference: &str, title: &str) -> Job {
    Job {
        source: Source::Board,
        source_ref: reference.to_owned(),
        title: title.to_owned(),
        company: "Acme".to_owned(),
        location: Some("Lisbon".to_owned()),
        work_mode: Some(WorkMode::Remote),
        board_remote: None,
        url: format!("https://example.com/jobs/{reference}"),
        posted_at: Some(today() - TimeDelta::days(1)),
        salary_text: None,
        description: "We build things".to_owned(),
        board: Some(BoardJob {
            slug: "alpha".to_owned(),
            ..BoardJob::default()
        }),
    }
}

#[test]
fn a_quiet_site_is_read_less_often_but_never_more_than_its_limits_allow() {
    let hour = TimeDelta::hours(1);
    assert_eq!(next_interval(hour, 0, false), hour);
    assert_eq!(next_interval(hour, 2, false), hour);
    assert_eq!(next_interval(hour, 3, false), hour * 2);
    assert_eq!(next_interval(hour, 6, false), hour * 4);
    assert_eq!(next_interval(hour, 10, false), hour * 8);
    assert_eq!(next_interval(hour, 50, false), TimeDelta::days(1));
    assert_eq!(next_interval(hour, 50, true), TimeDelta::days(7));
    // A site whose own limit is a week is never read sooner.
    let week = TimeDelta::days(7);
    assert_eq!(next_interval(week, 0, false), week);
    assert_eq!(next_interval(week, 50, false), week);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_is_read_into_the_store_and_a_second_read_keeps_only_what_changed(db: PgPool) {
    let server = MockServer::start().await;
    serve(
        &server,
        "/alpha",
        listing(&[("1", "Rust Engineer"), ("2", "Go Engineer")]),
    )
    .await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;

    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!((tick.due, tick.read, tick.new_jobs), (1, 1, 2));
    assert_eq!(open_jobs(&h.db, "alpha").await, 2);
    let (title, company, description, url): (String, String, String, String) = sqlx::query_as(
        "SELECT title, company, description, url FROM sources.store_jobs WHERE source_ref = '1'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(title, "Rust Engineer");
    assert_eq!(company, "Acme");
    assert_eq!(description, "We build things in Rust");
    assert_eq!(url, "https://example.com/jobs/1");
    let first = last(&h.db, "alpha").await;
    assert_eq!(first.status, "ok");
    assert_eq!((first.listed, first.new_jobs, first.quiet), (2, 2, 0));
    assert_eq!(first.http_status, Some(200));

    // Not before its next time: nothing is due, nothing is asked.
    let again = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(again.due, 0);
    assert_eq!(requests_to(&server, "/alpha").await, 1);

    // An hour later: one job changed, one is new, one is the same.
    server.reset().await;
    serve(
        &server,
        "/alpha",
        listing(&[
            ("1", "Staff Rust Engineer"),
            ("2", "Go Engineer"),
            ("3", "Ops Engineer"),
        ]),
    )
    .await;
    h.clock.advance(TimeDelta::minutes(61));
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!((tick.read, tick.new_jobs), (1, 1));
    let second = last(&h.db, "alpha").await;
    assert_eq!(
        (second.new_jobs, second.changed_jobs, second.closed_jobs),
        (1, 1, 0)
    );
    assert_eq!(open_jobs(&h.db, "alpha").await, 3);
    let seen: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_seen_at FROM sources.store_jobs WHERE source_ref = '2'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(
        seen,
        h.clock_now(),
        "a job seen again is touched, not rewritten"
    );

    // The registry row says it was read (the Boards page, the searches' pace).
    let fetched: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_fetch_at FROM sources.job_boards WHERE slug = 'alpha'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(fetched, Some(h.clock_now()));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_job_the_site_no_longer_lists_is_closed_then_deleted_and_comes_back_when_listed(
    db: PgPool,
) {
    let server = MockServer::start().await;
    serve(
        &server,
        "/alpha",
        listing(&[("1", "Rust Engineer"), ("2", "Go Engineer")]),
    )
    .await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;
    run_tick(&h.deps, &h.db).await.unwrap();

    server.reset().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    h.clock.advance(TimeDelta::minutes(61));
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(tick.closed_jobs, 1);
    assert_eq!(last(&h.db, "alpha").await.closed_jobs, 1);
    assert_eq!(open_jobs(&h.db, "alpha").await, 1);
    assert_eq!(
        count(&h.db, "SELECT count(*) FROM sources.store_jobs").await,
        2
    );

    // Listed again: open again.
    server.reset().await;
    serve(
        &server,
        "/alpha",
        listing(&[("1", "Rust Engineer"), ("2", "Go Engineer")]),
    )
    .await;
    h.clock.advance(TimeDelta::minutes(61));
    run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(open_jobs(&h.db, "alpha").await, 2);

    // Closed, and 14 days later gone.
    server.reset().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    h.clock.advance(TimeDelta::minutes(61));
    run_tick(&h.deps, &h.db).await.unwrap();
    let closed_at = h.clock_now();
    let early = clean(
        &h.db,
        closed_at + TimeDelta::days(13),
        TimeDelta::days(14),
        1_000,
    )
    .await
    .unwrap();
    assert_eq!(early.closed, 0, "a closed job is kept for 14 days");
    let late = clean(
        &h.db,
        closed_at + TimeDelta::days(15),
        TimeDelta::days(14),
        1_000,
    )
    .await
    .unwrap();
    assert_eq!(late.closed, 1);
    assert_eq!(
        count(&h.db, "SELECT count(*) FROM sources.store_jobs").await,
        1
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_read_that_stopped_at_its_page_limit_closes_nothing(db: PgPool) {
    let server = MockServer::start().await;
    // The answer names a next page, and the board reads one page.
    let mut page = listing(&[("1", "Rust Engineer"), ("2", "Go Engineer")]);
    page["next"] = json!("abc");
    serve(&server, "/alpha", page).await;
    let config = r#"cursor = { param = "cursor", from = "/next" }"#;
    seed(&db, &[board("alpha", &server, "approved", config, "")]).await;
    let h = harness(db, store_config()).await;
    run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(last(&h.db, "alpha").await.status, "partial");

    server.reset().await;
    let mut page = listing(&[("1", "Rust Engineer")]);
    page["next"] = json!("def");
    serve(&server, "/alpha", page).await;
    h.clock.advance(TimeDelta::minutes(61));
    run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(
        open_jobs(&h.db, "alpha").await,
        2,
        "what it did not list is not closed"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_full_read_that_would_close_most_of_a_site_is_taken_for_a_partial_answer(db: PgPool) {
    seed(
        &db,
        &[board(
            "alpha",
            &MockServer::start().await,
            "approved",
            "",
            "",
        )],
    )
    .await;
    let alpha = id_of(&db, "alpha").await;
    let many: Vec<Job> = (0..12).map(|n| job(&n.to_string(), "Engineer")).collect();
    let now = today();
    let kept = keep::keep(&db, alpha, &many, "tech", true, now, &limits())
        .await
        .unwrap();
    assert_eq!((kept.new, kept.closed), (12, 0));

    let few = &many[..2];
    let later = now + TimeDelta::hours(1);
    let kept = keep::keep(&db, alpha, few, "tech", true, later, &limits())
        .await
        .unwrap();
    assert!(kept.held_back);
    assert_eq!(kept.closed, 0);
    assert_eq!(open_jobs(&db, "alpha").await, 12);

    // A small drop is a real one.
    let most = &many[..9];
    let kept = keep::keep(&db, alpha, most, "tech", true, later, &limits())
        .await
        .unwrap();
    assert_eq!((kept.held_back, kept.closed), (false, 3));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn only_approved_sites_are_read_and_the_cap_per_minute_holds(db: PgPool) {
    let server = MockServer::start().await;
    for slug in ["alpha", "beta", "gamma", "delta"] {
        serve(
            &server,
            &format!("/{slug}"),
            listing(&[("1", "Rust Engineer")]),
        )
        .await;
    }
    seed(
        &db,
        &[
            board("alpha", &server, "approved", "", ""),
            board("beta", &server, "approved", "", ""),
            board("gamma", &server, "approved", "", ""),
            board("waiting", &server, "candidate", "", ""),
            board("resting", &server, "paused", "", ""),
        ],
    )
    .await;
    serve(&server, "/waiting", listing(&[("1", "Never read")])).await;
    serve(&server, "/resting", listing(&[("1", "Never read")])).await;
    let config = SourcesConfig {
        store_reads_per_minute: Some(2),
        ..store_config()
    };
    let h = harness(db, config).await;

    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 2);
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 1);
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    assert_eq!(
        count(&h.db, "SELECT count(*) FROM sources.store_reads").await,
        3
    );
    assert_eq!(requests_to(&server, "/waiting").await, 0);
    assert_eq!(requests_to(&server, "/resting").await, 0);

    // Paused again after approval: not read any more.
    sqlx::query("UPDATE sources.job_boards SET status = 'paused' WHERE slug = 'alpha'")
        .execute(&h.db)
        .await
        .unwrap();
    h.clock.advance(TimeDelta::hours(2));
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_sites_own_limits_come_first(db: PgPool) {
    let server = MockServer::start().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    serve(&server, "/beta", listing(&[("1", "Rust Engineer")])).await;
    seed(
        &db,
        &[
            // Two fetches a day: every 12 hours at the soonest.
            board("alpha", &server, "approved", "", "max_fetches_per_day = 2"),
            board("beta", &server, "approved", "", ""),
        ],
    )
    .await;
    let h = harness(db, store_config()).await;
    // A search read beta a few minutes ago: its polling limit is not over.
    sqlx::query("UPDATE sources.job_boards SET last_fetch_at = $1 WHERE slug = 'beta'")
        .bind(today() - TimeDelta::minutes(5))
        .execute(&h.db)
        .await
        .unwrap();
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(tick.due, 1, "beta waits for its polling interval");
    assert_eq!(requests_to(&server, "/beta").await, 0);
    let alpha = last(&h.db, "alpha").await;
    assert_eq!(alpha.next_read_at - alpha.at, TimeDelta::hours(12));

    h.clock.advance(TimeDelta::minutes(30));
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(tick.due, 1, "beta is due now; alpha, a day's limit, is not");
    assert_eq!(requests_to(&server, "/beta").await, 1);
    assert_eq!(requests_to(&server, "/alpha").await, 1);

    // A host that asked to be left alone is left alone.
    sqlx::query("UPDATE sources.job_boards SET backoff_until = $1 WHERE slug = 'beta'")
        .bind(h.clock_now() + TimeDelta::hours(5))
        .execute(&h.db)
        .await
        .unwrap();
    h.clock.advance(TimeDelta::hours(2));
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_conditional_request_keeps_every_job_when_nothing_changed(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/alpha"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/alpha"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_raw(
                    listing(&[("1", "Rust Engineer"), ("2", "Go Engineer")])
                        .to_string()
                        .into_bytes(),
                    "application/json",
                ),
        )
        .mount(&server)
        .await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;
    run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(last(&h.db, "alpha").await.etag.as_deref(), Some("\"v1\""));

    h.clock.advance(TimeDelta::minutes(61));
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!((tick.read, tick.not_modified), (0, 1));
    let second = last(&h.db, "alpha").await;
    assert_eq!(second.status, "not_modified");
    assert_eq!(second.http_status, Some(304));
    assert_eq!(
        second.etag.as_deref(),
        Some("\"v1\""),
        "its validator is kept"
    );
    assert_eq!(second.quiet, 1);
    assert_eq!(open_jobs(&h.db, "alpha").await, 2);
    let seen: DateTime<Utc> =
        sqlx::query_scalar("SELECT min(last_seen_at) FROM sources.store_jobs")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(
        seen,
        h.clock_now(),
        "jobs of a list that did not change are still seen"
    );
    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("if-none-match").is_none());
    assert!(requests[1].headers.get("if-none-match").is_some());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_quiet_site_is_asked_less_and_less_often(db: PgPool) {
    let server = MockServer::start().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;
    // One read with news, then reads with none.
    let mut gaps = Vec::new();
    for _ in 0..5 {
        run_tick(&h.deps, &h.db).await.unwrap();
        let line = last(&h.db, "alpha").await;
        gaps.push((line.quiet, (line.next_read_at - line.at).num_minutes()));
        h.clock
            .advance(line.next_read_at - line.at + TimeDelta::minutes(1));
    }
    // The hour of the instance's floor, then twice as long from the third
    // read without news.
    assert_eq!(gaps, vec![(0, 60), (1, 60), (2, 60), (3, 120), (4, 120)]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_that_asks_to_slow_down_is_left_alone_for_as_long_as_it_says(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/alpha"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7200"))
        .mount(&server)
        .await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;
    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(tick.failed, 1);
    let line = last(&h.db, "alpha").await;
    assert_eq!(line.status, "error");
    assert_eq!(line.error_kind.as_deref(), Some("rate_limited"));
    assert_eq!(line.http_status, Some(429));
    assert_eq!(line.next_read_at - line.at, TimeDelta::hours(2));
    let until: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT backoff_until FROM sources.job_boards WHERE slug = 'alpha'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(until, Some(h.clock_now() + TimeDelta::hours(2)));

    h.clock.advance(TimeDelta::minutes(90));
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    assert_eq!(requests_to(&server, "/alpha").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_job_past_its_expiry_is_dropped_when_the_terms_say_so(db: PgPool) {
    let server = MockServer::start().await;
    let mut body = listing(&[("1", "Expired"), ("2", "Open"), ("3", "Never expires")]);
    body["jobs"][0]["expires"] = json!("2026-10-20T00:00:00Z");
    body["jobs"][1]["expires"] = json!("2026-11-20T00:00:00Z");
    serve(&server, "/alpha", body.clone()).await;
    serve(&server, "/beta", body).await;
    seed(
        &db,
        &[
            board("alpha", &server, "approved", "", "drop_expired = true"),
            // A board whose terms do not ask for it keeps no expiry at all.
            board("beta", &server, "approved", "", ""),
        ],
    )
    .await;
    let h = harness(db, store_config()).await;
    run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!(open_jobs(&h.db, "alpha").await, 2);
    assert_eq!(last(&h.db, "alpha").await.dropped_jobs, 1);
    assert_eq!(open_jobs(&h.db, "beta").await, 3);
    let expires: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT j.expires_at FROM sources.store_jobs j JOIN sources.job_boards b ON b.id = j.board_id
          WHERE b.slug = 'alpha' AND j.source_ref = '2'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert!(expires.is_some());

    // The day after it expires, it goes.
    let cleaned = clean(
        &h.db,
        today() + TimeDelta::days(28),
        TimeDelta::days(14),
        1_000,
    )
    .await
    .unwrap();
    assert_eq!(cleaned.expired, 1);
    assert_eq!(open_jobs(&h.db, "alpha").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn old_reads_go_but_each_sites_latest_stays_and_the_biggest_listings_are_cut(db: PgPool) {
    let server = MockServer::start().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    seed(
        &db,
        &[
            board("alpha", &server, "approved", "", ""),
            board("beta", &server, "paused", "", ""),
        ],
    )
    .await;
    let h = harness(db, store_config()).await;
    for _ in 0..3 {
        run_tick(&h.deps, &h.db).await.unwrap();
        h.clock.advance(TimeDelta::hours(2));
    }
    assert_eq!(
        count(&h.db, "SELECT count(*) FROM sources.store_reads").await,
        3
    );
    let cleaned = clean(
        &h.db,
        h.clock_now() + TimeDelta::days(10),
        TimeDelta::days(14),
        1_000,
    )
    .await
    .unwrap();
    assert_eq!(cleaned.reads, 2);
    assert_eq!(
        count(&h.db, "SELECT count(*) FROM sources.store_reads").await,
        1
    );

    // A site over its limit keeps the newest.
    let beta = id_of(&h.db, "beta").await;
    let jobs: Vec<Job> = (0..5)
        .map(|n| Job {
            posted_at: Some(today() - TimeDelta::days(n)),
            ..job(&format!("n{n}"), "Engineer")
        })
        .collect();
    let small = Limits {
        description_chars: 100,
        max_per_site: 3,
    };
    let kept = keep::keep(&h.db, beta, &jobs, "", true, today(), &small)
        .await
        .unwrap();
    assert_eq!((kept.new, kept.dropped), (3, 2));
    let oldest: Option<String> = sqlx::query_scalar(
        "SELECT source_ref FROM sources.store_jobs WHERE source_ref LIKE 'n%'
          ORDER BY posted_at LIMIT 1",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(oldest.as_deref(), Some("n2"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_company_page_is_read_through_its_system_once_a_day_and_never_when_its_system_waits(
    db: PgPool,
) {
    let greenhouse = MockServer::start().await;
    let jobs: Vec<Value> = (0..3)
        .map(|n| {
            json!({
                "id": 1000 + n,
                "title": format!("Engineer {n}"),
                "absolute_url": format!("https://boards.greenhouse.io/acme/jobs/{}", 1000 + n),
                "location": { "name": "Remote" },
                "first_published": "2026-10-20T10:00:00Z",
            })
        })
        .collect();
    serve(&greenhouse, "/v1/boards/acme/jobs", json!({ "jobs": jobs })).await;
    let config = SourcesConfig {
        greenhouse_url: greenhouse.uri().parse().unwrap(),
        ..store_config()
    };
    let h = harness(db, config).await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    registry::register(
        &h.db,
        &[CompanyLink {
            source: Source::Greenhouse,
            token: "acme".to_owned(),
            eu: false,
        }],
        "an apply link on Board alpha",
        today(),
    )
    .await
    .unwrap();
    let approve = |kind: &'static str| {
        let db = h.db.clone();
        async move {
            sqlx::query(
                "UPDATE sources.job_boards SET status = 'approved', approved_by = 'app',
                        approved_at = now() WHERE kind = $1 AND system = 'greenhouse'",
            )
            .bind(kind)
            .execute(&db)
            .await
            .unwrap();
        }
    };

    // A page of a system that waits is not read, nor is a page that waits.
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    approve("company_page").await;
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    approve("hiring_system").await;
    // Its system's robots rules forbid reading company pages.
    sqlx::query(
        "UPDATE sources.job_boards SET blockers = '[{\"code\": \"robots_disallowed\", \"detail\": \"x\"}]'
          WHERE kind = 'hiring_system' AND system = 'greenhouse'",
    )
    .execute(&h.db)
    .await
    .unwrap();
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    sqlx::query("UPDATE sources.job_boards SET blockers = '[]' WHERE kind = 'hiring_system'")
        .execute(&h.db)
        .await
        .unwrap();

    let tick = run_tick(&h.deps, &h.db).await.unwrap();
    assert_eq!((tick.due, tick.read, tick.new_jobs), (1, 1, 3));
    let slug: String = sqlx::query_scalar(
        "SELECT slug FROM sources.job_boards WHERE kind = 'company_page' AND system = 'greenhouse'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(open_jobs(&h.db, &slug).await, 3);
    let line = last(&h.db, &slug).await;
    assert_eq!(line.next_read_at - line.at, TimeDelta::days(1));
    h.clock.advance(TimeDelta::hours(5));
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 0);
    h.clock.advance(TimeDelta::hours(20));
    assert_eq!(run_tick(&h.deps, &h.db).await.unwrap().due, 1);
    assert_eq!(requests_to(&greenhouse, "/v1/boards/acme/jobs").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn every_read_registers_the_company_pages_its_apply_links_name(db: PgPool) {
    let server = MockServer::start().await;
    let mut body = listing(&[("1", "Rust Engineer")]);
    body["jobs"][0]["url"] = json!("https://jobs.lever.co/northlight-labs/0b6d1c2e-4f1c-4a55-8a1d");
    serve(&server, "/alpha", body).await;
    seed(&db, &[board("alpha", &server, "approved", "", "")]).await;
    let h = harness(db, store_config()).await;
    registry::ensure_systems(&h.db, today()).await.unwrap();
    run_tick(&h.deps, &h.db).await.unwrap();
    let pages: Vec<String> = sqlx::query_scalar(
        "SELECT company_token FROM sources.job_boards WHERE kind = 'company_page'",
    )
    .fetch_all(&h.db)
    .await
    .unwrap();
    assert_eq!(pages, vec!["northlight-labs".to_owned()]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_search_reads_open_jobs_of_approved_sites_by_what_it_filters_on(db: PgPool) {
    let server = MockServer::start().await;
    seed(
        &db,
        &[
            board("alpha", &server, "approved", "", ""),
            board("beta", &server, "approved", "", ""),
        ],
    )
    .await;
    let alpha = id_of(&db, "alpha").await;
    let beta = id_of(&db, "beta").await;
    let now = today();
    let mut onsite = job("2", "Backend Developer");
    onsite.work_mode = Some(WorkMode::Onsite);
    onsite.posted_at = Some(now - TimeDelta::days(20));
    let mut europe = job("3", "Platform Engineer");
    europe.board = Some(BoardJob {
        slug: "alpha".to_owned(),
        regions: vec!["Europe".to_owned()],
        ..BoardJob::default()
    });
    let mut us_only = job("4", "Rust Engineer");
    us_only.board = Some(BoardJob {
        slug: "alpha".to_owned(),
        regions: vec!["United States".to_owned()],
        ..BoardJob::default()
    });
    let mut unknown_mode = job("5", "Rust Developer");
    unknown_mode.work_mode = None;
    keep::keep(
        &db,
        alpha,
        &[
            job("1", "Senior Rust Engineer"),
            onsite,
            europe,
            us_only,
            unknown_mode,
        ],
        "tech remote",
        true,
        now,
        &limits(),
    )
    .await
    .unwrap();
    keep::keep(
        &db,
        beta,
        &[job("1", "Designer")],
        "design",
        true,
        now,
        &limits(),
    )
    .await
    .unwrap();
    let titles = |found: Vec<StoredJob>| {
        let mut titles: Vec<String> = found.into_iter().map(|j| j.title).collect();
        titles.sort();
        titles
    };

    let all = find(&db, &StoreQuery::default(), now).await.unwrap();
    assert_eq!(all.len(), 6);
    // Words of the person's roles and skills: terms are alternatives, the
    // words of one term all needed; the site's tags count.
    let rust = ["rust engineer".to_owned(), "designer".to_owned()];
    let found = find(
        &db,
        &StoreQuery {
            text: &rust,
            ..StoreQuery::default()
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        titles(found),
        ["Designer", "Rust Engineer", "Senior Rust Engineer"]
    );
    let tags = ["design".to_owned()];
    assert_eq!(
        find(
            &db,
            &StoreQuery {
                text: &tags,
                ..StoreQuery::default()
            },
            now
        )
        .await
        .unwrap()
        .len(),
        1,
        "the site's tags are searched"
    );
    // Posted since.
    let recent = StoreQuery {
        posted_since: Some(now - TimeDelta::days(7)),
        ..StoreQuery::default()
    };
    assert_eq!(find(&db, &recent, now).await.unwrap().len(), 5);
    // Work mode: remote ones, and those that say nothing.
    let remote = [WorkMode::Remote];
    let found = find(
        &db,
        &StoreQuery {
            work_modes: &remote,
            ..StoreQuery::default()
        },
        now,
    )
    .await
    .unwrap();
    assert!(!titles(found).contains(&"Backend Developer".to_owned()));
    // Regions: open to Europe, or naming none.
    let regions = ["europe".to_owned()];
    let found = find(
        &db,
        &StoreQuery {
            regions: &regions,
            ..StoreQuery::default()
        },
        now,
    )
    .await
    .unwrap();
    let found = titles(found);
    assert!(found.contains(&"Platform Engineer".to_owned()));
    assert!(
        !found.contains(&"Rust Engineer".to_owned()),
        "open to the United States only"
    );
    assert!(
        found.contains(&"Senior Rust Engineer".to_owned()),
        "its site names no region"
    );
    // The limit; the newest first.
    let one = find(
        &db,
        &StoreQuery {
            limit: 1,
            ..StoreQuery::default()
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(one.len(), 1);
    // Only what is new since the last nightly search.
    let later = now + TimeDelta::hours(5);
    keep::keep(
        &db,
        beta,
        &[job("1", "Designer"), job("2", "Illustrator")],
        "design",
        true,
        later,
        &limits(),
    )
    .await
    .unwrap();
    let new = StoreQuery {
        first_seen_since: Some(now + TimeDelta::hours(1)),
        ..StoreQuery::default()
    };
    assert_eq!(
        titles(find(&db, &new, later).await.unwrap()),
        ["Illustrator"]
    );

    // A closed job, a paused site and an expired job are out of view; the
    // job is a job as the readers give it.
    sqlx::query("UPDATE sources.job_boards SET status = 'paused' WHERE slug = 'beta'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE sources.store_jobs SET closed_at = $1 WHERE source_ref = '2' AND board_id = $2",
    )
    .bind(now)
    .bind(alpha)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE sources.store_jobs SET expires_at = $1 WHERE source_ref = '5'")
        .bind(now - TimeDelta::days(1))
        .execute(&db)
        .await
        .unwrap();
    let left = find(&db, &StoreQuery::default(), now).await.unwrap();
    assert_eq!(left.len(), 3);
    let read = left
        .into_iter()
        .find(|j| j.source_ref == "3")
        .unwrap()
        .into_job();
    assert_eq!(read.source, Source::Board);
    assert_eq!(read.title, "Platform Engineer");
    let board = read.board.unwrap();
    assert_eq!(board.slug, "alpha");
    assert_eq!(board.regions, vec!["Europe".to_owned()]);
    assert_eq!(board.links[0].name, "Board alpha");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_owner_sees_how_the_store_does_and_nobody_else_does(db: PgPool) {
    let server = MockServer::start().await;
    serve(&server, "/alpha", listing(&[("1", "Rust Engineer")])).await;
    Mock::given(method("GET"))
        .and(path("/beta"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    seed(
        &db,
        &[
            board("alpha", &server, "approved", "", ""),
            board("beta", &server, "approved", "", ""),
        ],
    )
    .await;
    let fake = Fake::ready();
    fake.signed_in_as("someone@example.com");
    let config = SourcesConfig {
        board_owners: Some(vec!["owner@example.com".into()]),
        ..store_config()
    };
    let h = Harness::new(db, config, None, fake.clone()).await;
    run_tick(&h.deps, &h.db).await.unwrap();

    let (status, _) = h.get("/api/v1/boards/store").await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    fake.signed_in_as("owner@example.com");
    let (status, body) = h.get("/api/v1/boards/store").await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["on"], true);
    assert_eq!(body["jobs_stored"], 1);
    assert_eq!(body["new_7d"], 1);
    assert_eq!(body["sites_read_today"], 1);
    assert_eq!(body["reads_today"], 2);
    assert_eq!(body["failures_today"], 1);
    assert_eq!(body["sites_failing"], 1);
    assert!(body["last_read_at"].is_string());
}
