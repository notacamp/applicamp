use chrono::TimeDelta;
use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, path_regex},
};

use super::*;
use crate::{
    SourcesConfig,
    testkit::{Fake, Harness, boards_config, requests_to, today},
};

/// One job block of the list, in the documented form.
fn job(company: &str, code: &str, date: &str, url: &str, site: Option<&str>) -> String {
    format!(
        "<job><title><![CDATA[Role]]></title><date><![CDATA[{date}]]></date>\
         <referencenumber><![CDATA[{code}]]></referencenumber><url><![CDATA[{url}]]></url>\
         <company><![CDATA[{company}]]></company><city><![CDATA[Lisbon]]></city>\
         <description><![CDATA[<p>Invented.</p>]]></description>{}</job>",
        site.map(|s| format!("<website><![CDATA[{s}]]></website>"))
            .unwrap_or_default()
    )
}

fn list(jobs: &[String]) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><source><publisher>Workable</publisher>{}</source>",
        jobs.join("\n")
    )
}

/// Invented companies: Meridian (two recent jobs), Old Lantern (only an old
/// one), Harbour (one recent), Orbit (recent, on the exclusion list).
fn invented() -> String {
    let link = |code: &str| format!("https://apply.workable.com/j/{code}");
    list(&[
        job(
            "Meridian Bakery",
            "AAAA111111",
            "Fri, 30 Oct 2025 09:00:00 UTC",
            &link("AAAA111111"),
            None,
        ),
        job(
            "Meridian Bakery",
            "AAAA222222",
            "Thu, 22 Oct 2026 09:00:00 UTC",
            &link("AAAA222222"),
            Some("https://meridian.example.com"),
        ),
        job(
            "Old Lantern Press",
            "BBBB111111",
            "Wed, 28 Jul 2021 10:24:17 UTC",
            &link("BBBB111111"),
            None,
        ),
        job(
            "Harbour Works",
            "CCCC111111",
            "Mon, 19 Oct 2026 09:00:00 UTC",
            &link("CCCC111111"),
            None,
        ),
        job(
            "Orbit Labs",
            "DDDD111111",
            "Mon, 19 Oct 2026 09:00:00 UTC",
            &link("DDDD111111"),
            Some("https://www.orbit.example.com"),
        ),
    ])
}

/// The apply host's fake: every short link redirects to the company's page.
async fn apply_host() -> MockServer {
    let server = MockServer::start().await;
    for (code, token) in [
        ("AAAA222222", "meridian-bakery"),
        ("CCCC111111", "harbour-works"),
        ("DDDD111111", "orbit-labs"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/j/{code}")))
            .respond_with(
                ResponseTemplate::new(301).insert_header("location", format!("/{token}/j/{code}")),
            )
            .mount(&server)
            .await;
    }
    server
}

async fn feed(body: String) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/boards/workable.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .insert_header("last-modified", "Wed, 04 Nov 2026 16:18:18 GMT")
                .set_body_raw(body.into_bytes(), "application/xml"),
        )
        .mount(&server)
        .await;
    server
}

fn config(feed: &MockServer, apply: &MockServer) -> SourcesConfig {
    SourcesConfig {
        workable_url: apply.uri().parse().unwrap(),
        workable_list_url: format!("{}/boards/workable.xml", feed.uri())
            .parse()
            .unwrap(),
        company_pages: Some(false),
        ..boards_config()
    }
}

async fn pages(db: &PgPool) -> Vec<(String, String, String, Option<i32>, Option<String>)> {
    sqlx::query_as(
        "SELECT company_token, status, found_through, list_jobs, approved_by
           FROM sources.job_boards WHERE kind = 'company_page' ORDER BY company_token",
    )
    .fetch_all(db)
    .await
    .unwrap()
}

async fn approve_workable(db: &PgPool) {
    sqlx::query(
        "UPDATE sources.job_boards SET status = 'approved', approved_by = 'owner'
          WHERE kind = 'hiring_system' AND system = 'workable'",
    )
    .execute(db)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../../migrations")]
async fn companies_with_a_recent_job_become_company_pages_that_wait_with_their_system(db: PgPool) {
    let (feed, apply) = (feed(invented()).await, apply_host().await);
    let mut cfg = config(&feed, &apply);
    cfg.workable_list_exclude = Some(vec!["orbit.example.com".to_owned()]);
    let h = Harness::new(db, cfg, None, Fake::ready()).await;

    let summary = run(&h.deps, &h.db).await.unwrap();
    assert_eq!(summary.outcome, "complete");
    assert_eq!(summary.jobs, 5);
    assert_eq!(summary.companies, 4);
    // Meridian and Harbour are looked up; Old Lantern has no recent job and
    // Orbit is excluded: neither costs a request.
    assert_eq!(summary.looked_up, 2, "{summary:?}");
    assert_eq!(summary.added, 2);
    assert_eq!(
        summary.approved, 0,
        "the Workable system waits for the owner"
    );
    assert_eq!(requests_to(&apply, "/j/").await, 2);

    let found = pages(&h.db).await;
    assert_eq!(
        found,
        vec![
            (
                "harbour-works".to_owned(),
                "candidate".to_owned(),
                "the Workable list".to_owned(),
                Some(1),
                None
            ),
            (
                "meridian-bakery".to_owned(),
                "candidate".to_owned(),
                "the Workable list".to_owned(),
                Some(2),
                None
            ),
        ]
    );
    let name: String = sqlx::query_scalar(
        "SELECT name FROM sources.job_boards WHERE company_token = 'meridian-bakery'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(name, "Meridian Bakery");
    // Only the count is kept: no job, no description.
    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_schema = 'sources' AND table_name = 'workable_list'
            AND column_name IN ('title', 'description', 'url')",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(columns, 0);

    // The owner approves Workable: the pages follow, with no sample read.
    approve_workable(&h.db).await;
    assert_eq!(approve_listed(&h.db, today()).await.unwrap(), 2);
    let found = pages(&h.db).await;
    assert!(
        found
            .iter()
            .all(|p| p.1 == "approved" && p.4.as_deref() == Some("app"))
    );
    let notes: String = sqlx::query_scalar(
        "SELECT report->>'notes' FROM sources.job_boards WHERE company_token = 'meridian-bakery'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert!(notes.contains("no sample of its page was read"), "{notes}");
    assert_eq!(requests_to(&apply, "/api/").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn pages_of_the_list_are_approved_by_the_app_when_their_system_is_already_approved(
    db: PgPool,
) {
    let (feed, apply) = (feed(invented()).await, apply_host().await);
    let h = Harness::new(db, config(&feed, &apply), None, Fake::ready()).await;
    company_pages::ensure_systems(&h.db, today()).await.unwrap();
    approve_workable(&h.db).await;
    let summary = run(&h.deps, &h.db).await.unwrap();
    // Orbit is not excluded here.
    assert_eq!(summary.added, 3);
    assert_eq!(summary.approved, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_declined_system_is_not_read_and_a_run_within_a_day_is_skipped(db: PgPool) {
    let (feed, apply) = (feed(invented()).await, apply_host().await);
    let h = Harness::new(db, config(&feed, &apply), None, Fake::ready()).await;
    assert_eq!(run(&h.deps, &h.db).await.unwrap().outcome, "complete");
    // Within a day: no download.
    h.clock.advance(TimeDelta::hours(3));
    assert_eq!(run(&h.deps, &h.db).await.unwrap().outcome, "skipped");
    assert_eq!(requests_to(&feed, "/boards/workable.xml").await, 1);
    // Declined: not read at all.
    h.clock.advance(TimeDelta::days(2));
    sqlx::query(
        "UPDATE sources.job_boards SET status = 'declined'
          WHERE kind = 'hiring_system' AND system = 'workable'",
    )
    .execute(&h.db)
    .await
    .unwrap();
    assert_eq!(run(&h.deps, &h.db).await.unwrap().outcome, "skipped");
    assert_eq!(requests_to(&feed, "/boards/workable.xml").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_next_download_is_conditional_and_a_304_changes_nothing(db: PgPool) {
    let apply = apply_host().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/boards/workable.xml"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/boards/workable.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .insert_header("last-modified", "Wed, 04 Nov 2026 16:18:18 GMT")
                .set_body_raw(invented().into_bytes(), "application/xml"),
        )
        .mount(&server)
        .await;
    let h = Harness::new(db, config(&server, &apply), None, Fake::ready()).await;
    assert_eq!(run(&h.deps, &h.db).await.unwrap().outcome, "complete");
    h.clock.advance(TimeDelta::hours(25));
    let second = run(&h.deps, &h.db).await.unwrap();
    assert_eq!(second.outcome, "unchanged");
    assert_eq!(second.bytes, 0);
    let asked = server.received_requests().await.unwrap();
    let last = &asked[asked.len() - 1];
    assert_eq!(
        last.headers.get("if-modified-since").and_then(|v| v.to_str().ok()),
        Some("Wed, 04 Nov 2026 16:18:18 GMT")
    );
    // The cache validators are kept for the day after.
    let (etag, outcome): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT etag, outcome FROM sources.workable_list_state")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(etag.as_deref(), Some("\"v1\""));
    assert_eq!(outcome.as_deref(), Some("unchanged"));
    assert_eq!(pages(&h.db).await.len(), 3, "the pages stay");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_download_cut_by_the_byte_cap_keeps_what_it_read(db: PgPool) {
    let apply = apply_host().await;
    let many: Vec<String> = (0..40)
        .map(|i| {
            job(
                &format!("Company {i}"),
                &format!("ZZ{i:08}"),
                "Mon, 19 Oct 2026 09:00:00 UTC",
                &format!("https://apply.workable.com/j/ZZ{i:08}"),
                None,
            )
        })
        .collect();
    let feed = feed(list(&many)).await;
    let mut cfg = config(&feed, &apply);
    cfg.workable_list_max_bytes = Some(2_000);
    cfg.workable_list_lookups = Some(0);
    let h = Harness::new(db, cfg, None, Fake::ready()).await;
    let summary = run(&h.deps, &h.db).await.unwrap();
    assert_eq!(summary.outcome, "capped");
    assert!(summary.bytes <= 2_000, "{}", summary.bytes);
    assert!(summary.jobs >= 1 && summary.jobs < 40, "{}", summary.jobs);
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.workable_list")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(kept, i64::try_from(summary.jobs).unwrap());
    // A cut download is not the one the next request is conditional on.
    let etag: Option<String> = sqlx::query_scalar("SELECT etag FROM sources.workable_list_state")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(etag, None);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_429_is_honoured_and_robots_txt_is_respected(db: PgPool) {
    let apply = apply_host().await;
    let limited = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/boards/"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7200"))
        .mount(&limited)
        .await;
    let h = Harness::new(db.clone(), config(&limited, &apply), None, Fake::ready()).await;
    assert_eq!(run(&h.deps, &h.db).await.unwrap().outcome, "rate_limited");
    let until: DateTime<Utc> =
        sqlx::query_scalar("SELECT not_before FROM sources.workable_list_state")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(until, today() + TimeDelta::hours(2));

    // robots.txt that forbids the list: not asked.
    let closed = feed(invented()).await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"User-agent: *\nDisallow: /boards/\n".to_vec(),
            "text/plain",
        ))
        .mount(&closed)
        .await;
    sqlx::query("DELETE FROM sources.workable_list_state")
        .execute(&db)
        .await
        .unwrap();
    let h2 = Harness::new(db, config(&closed, &apply), None, Fake::ready()).await;
    assert_eq!(run(&h2.deps, &h2.db).await.unwrap().outcome, "refused");
    assert_eq!(requests_to(&closed, "/boards/workable.xml").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn counts_are_per_run_and_the_upsert_is_idempotent(db: PgPool) {
    let (feed, apply) = (feed(invented()).await, apply_host().await);
    let h = Harness::new(db, config(&feed, &apply), None, Fake::ready()).await;
    let now = today();
    let run_a = Uuid::now_v7();
    let mut map = HashMap::new();
    let item = |code: &str, date: DateTime<Utc>| feed::Item {
        company: "Acme Pastry".to_owned(),
        code: code.to_owned(),
        url: format!("https://apply.workable.com/acme-pastry/j/{code}/"),
        date: Some(date),
        website: None,
    };
    count(&mut map, item("OLD1", now - TimeDelta::days(400)), now);
    count(&mut map, item("NEW1", now - TimeDelta::days(3)), now);
    flush(&h.db, run_a, &mut map, now).await.unwrap();
    // Counted in two batches of the same run: added up.
    count(&mut map, item("NEW2", now - TimeDelta::days(1)), now);
    flush(&h.db, run_a, &mut map, now).await.unwrap();
    let (jobs, recent, code, token): (i32, i32, String, Option<String>) =
        sqlx::query_as("SELECT jobs, recent_jobs, sample_code, token FROM sources.workable_list")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!((jobs, recent, code.as_str()), (3, 2, "NEW2"));
    assert_eq!(
        token.as_deref(),
        Some("acme-pastry"),
        "a link with the token needs no request"
    );
    // The next run starts again from its own count.
    count(&mut map, item("NEW3", now - TimeDelta::days(1)), now);
    flush(&h.db, Uuid::now_v7(), &mut map, now + TimeDelta::days(1))
        .await
        .unwrap();
    let jobs: i32 = sqlx::query_scalar("SELECT jobs FROM sources.workable_list")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(jobs, 1);

    company_pages::ensure_systems(&h.db, now).await.unwrap();
    assert_eq!(promote(&h.deps, &h.db, now).await.unwrap(), 1);
    // Nothing changed since: nothing written, and a later sighting updates
    // the row in place rather than adding a second.
    assert_eq!(promote(&h.deps, &h.db, now).await.unwrap(), 0);
    sqlx::query(
        "UPDATE sources.workable_list SET last_seen_at = last_seen_at + interval '1 day', jobs = 9",
    )
    .execute(&h.db)
    .await
    .unwrap();
    assert_eq!(
        promote(&h.deps, &h.db, now + TimeDelta::days(1))
            .await
            .unwrap(),
        0
    );
    let found = pages(&h.db).await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].3, Some(9));
}

#[test]
fn a_redirect_names_the_company() {
    let base: Url = "https://apply.workable.com/j/AAAA".parse().unwrap();
    assert_eq!(
        token_of_redirect(&base, "/acme/j/AAAA").as_deref(),
        Some("acme")
    );
    assert_eq!(
        token_of_redirect(&base, "https://apply.workable.com/acme-2/j/AAAA/").as_deref(),
        Some("acme-2")
    );
    assert_eq!(token_of_redirect(&base, "/j/AAAA"), None);
    assert_eq!(token_of_redirect(&base, "/"), None);
}

#[test]
fn the_exclusion_list_matches_names_and_sites() {
    let config = SourcesConfig {
        workable_list_exclude: Some(vec!["orbit.example.com".to_owned(), "acme labs".to_owned()]),
        ..SourcesConfig::default()
    };
    assert!(config.workable_list_excludes("Whoever", Some("https://www.orbit.example.com/jobs")));
    assert!(config.workable_list_excludes("Acme Labs", None));
    assert!(!config.workable_list_excludes("Meridian", Some("https://meridian.example.com")));
    assert!(!SourcesConfig::default().workable_list_excludes("Acme Labs", None));
}
