use chrono::{TimeZone as _, Utc};
use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use super::*;
use crate::testkit::{Fake, Harness, boards_config, requests_to, seed_fake_boards, today};

#[test]
fn the_list_gives_names_links_and_sections() {
    let markdown = "# Awesome Job Boards\n\n## Contents\n\n- [Remote](#remote)\n\n\
                    ## General\n\n- [LinkedIn](https://www.linkedin.com/)\n\n\
                    ## Remote\n\n- [Example Remote](https://remote.example/) - Remote jobs.\n\
                    * [Other](https://other.example/jobs)\n\n### Germany\n\n- [Beispiel](https://beispiel.example)\n";
    let entries = parse_list(markdown);
    let names: Vec<(&str, &str)> = entries
        .iter()
        .map(|e| (e.name.as_str(), e.section.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("LinkedIn", "General"),
            ("Example Remote", "Remote"),
            ("Other", "Remote"),
            ("Beispiel", "Germany")
        ]
    );
    assert!(never("linkedin.com") && never("de.indeed.com") && never("glassdoor.co.uk"));
    assert!(!never("remote.example"));
}

#[test]
fn it_runs_on_mondays() {
    let tuesday = Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap();
    assert_eq!(
        next_run(tuesday),
        Utc.with_ymd_and_hms(2026, 11, 2, 4, 0, 0).unwrap()
    );
    let monday_early = Utc.with_ymd_and_hms(2026, 11, 2, 3, 0, 0).unwrap();
    assert_eq!(
        next_run(monday_early),
        Utc.with_ymd_and_hms(2026, 11, 2, 4, 0, 0).unwrap()
    );
    let monday_late = Utc.with_ymd_and_hms(2026, 11, 2, 5, 0, 0).unwrap();
    assert_eq!(
        next_run(monday_late),
        Utc.with_ymd_and_hms(2026, 11, 9, 4, 0, 0).unwrap()
    );
}

#[test]
fn terms_sentences_and_configs_are_proposed() {
    let terms = "<h1>Terms</h1><p>Welcome to our site. You may not scrape or crawl this \
                 site with automated tools. Please link back to the original posting when you \
                 share a job. Cookies keep you signed in.</p>";
    let sentences = terms_sentences(terms);
    assert_eq!(sentences.len(), 2, "{sentences:?}");
    assert!(sentences[0].starts_with("You may not scrape"));
    let endpoint: Url = "https://board.example/api".parse().unwrap();
    let remoteok_like = serde_json::json!([
        { "legal": "Terms of the API" },
        { "id": "1", "position": "Engineer", "company": "Example Co", "url": "https://board.example/1" }
    ]);
    let config = propose_json(&endpoint, &remoteok_like).unwrap();
    assert!(config.skip_first);
    assert_eq!(config.fields.title.as_deref(), Some("/position"));
    assert_eq!(config.fields.company.as_deref(), Some("/company"));
    let wrapped = serde_json::json!({ "meta": 1, "jobs": [
        { "id": 7, "title": "Engineer", "companyName": "Example Co", "link": "https://board.example/7" }
    ]});
    let config = propose_json(&endpoint, &wrapped).unwrap();
    assert_eq!(config.items.as_deref(), Some("/jobs"));
    assert_eq!(config.fields.url.as_deref(), Some("/link"));
    assert!(propose_json(&endpoint, &serde_json::json!({ "ok": true })).is_none());
}

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel>
<item><title>Remote Senior Engineer</title><region>Anywhere in the World</region><link>https://remote.example/jobs/1</link><guid>1</guid><pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate><description>Invented.</description></item>
<item><title>Remote Staff Engineer</title><region>Worldwide</region><link>https://remote.example/jobs/2</link><guid>2</guid><pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate><description>Invented.</description></item>
<item><title>Remote Platform Engineer</title><region>Anywhere</region><link>https://remote.example/jobs/3</link><guid>3</guid><pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate><description>Invented.</description></item>
</channel></rss>"#;

/// A board's site: robots rules, a homepage linking its feed and terms,
/// the terms, the feed; and the list naming it (and LinkedIn).
async fn a_board_site() -> MockServer {
    let server = MockServer::start().await;
    let list = format!(
        "## General\n\n- [LinkedIn](https://www.linkedin.com/)\n\n## Remote\n\n\
         - [Example Remote]({}/) - Remote jobs.\n",
        server.uri()
    );
    let page =
        |body: String, kind: &str| ResponseTemplate::new(200).set_body_raw(body.into_bytes(), kind);
    Mock::given(method("GET"))
        .and(path("/list.md"))
        .respond_with(page(list, "text/markdown"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(page(
            "User-agent: *\nDisallow: /private\n".into(),
            "text/plain",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(page(
            r#"<html lang="en"><head><link rel="alternate" type="application/rss+xml" href="/feed.xml"></head>
               <body><a href="/terms">Terms of service</a><a href="/about">About</a></body></html>"#
                .into(),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/terms"))
        .respond_with(page(
            "<p>You may not scrape this site with automated tools. Please link back to the \
             original posting. We like cookies.</p>"
                .into(),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(page(FEED.into(), "application/rss+xml"))
        .mount(&server)
        .await;
    server
}

#[sqlx::test(migrations = "../../../migrations")]
async fn discovery_checks_a_new_board_and_puts_it_in_the_queue(db: PgPool) {
    let site = a_board_site().await;
    let mut settings = boards_config();
    settings.discovery_list_url = format!("{}/list.md", site.uri()).parse().unwrap();
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    discover(&h.deps, &h.db).await.unwrap();

    let boards = job_boards::all(&h.db).await.unwrap();
    assert_eq!(boards.len(), 1, "LinkedIn is never added");
    let board = &boards[0];
    assert_eq!(board.status(), Status::Candidate);
    assert_eq!(board.origin(), job_boards::Origin::Discovery);
    assert_eq!(board.kind(), Some(Kind::Rss));
    assert_eq!(
        board.priority(),
        job_boards::Priority::High,
        "remote, open worldwide"
    );
    let report = board.report().unwrap();
    assert!(report.robots.found);
    assert_eq!(report.robots.rules.len(), 1);
    assert_eq!(report.language.as_deref(), Some("en"));
    assert_eq!(
        report.terms_sentences.len(),
        2,
        "{:?}",
        report.terms_sentences
    );
    assert_eq!(report.sample.len(), 3);
    assert_eq!(report.found_kind, Some(Kind::Rss));
    assert!(
        report.flags.iter().any(|f| f.contains("scraping")),
        "{:?}",
        report.flags
    );
    let config = board.config().unwrap();
    assert!(config.endpoint.unwrap().as_str().ends_with("/feed.xml"));
    // Every path once; the jobs never read into a search.
    for page in ["/robots.txt", "/terms", "/feed.xml", "/list.md"] {
        assert_eq!(requests_to(&site, page).await, 1, "{page}");
    }
    assert!(job_boards::approved(&h.db).await.unwrap().is_empty());
    // The sample ranks it; the check is a line of the Scanned view.
    let samples: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.board_jobs WHERE sample")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(samples, 3);
    let (_, page) = h.get("/api/v1/boards/scans").await;
    let run = &page["runs"][0];
    assert_eq!(run["kind"], "discovery");
    assert_eq!(run["lines"][0]["outcome"], "checked");
    let note = run["lines"][0]["note"].as_str().unwrap();
    assert!(note.starts_with("Feed at /feed.xml"), "{note}");

    // A second run finds nothing new.
    discover(&h.deps, &h.db).await.unwrap();
    assert_eq!(job_boards::all(&h.db).await.unwrap().len(), 1);
    assert_eq!(
        requests_to(&site, "/feed.xml").await,
        1,
        "never fetched again"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn approved_boards_are_rechecked_and_changes_flagged(db: PgPool) {
    let site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /remoteok/\n"),
        )
        .mount(&site)
        .await;
    let mut settings = boards_config();
    settings.discovery_list_url = format!("{}/missing.md", site.uri()).parse().unwrap();
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    seed_fake_boards(&h.db, &site, &["remoteok"]).await;
    discover(&h.deps, &h.db).await.unwrap();
    let board = job_boards::approved(&h.db).await.unwrap().remove(0);
    let report = board.report().unwrap();
    assert!(
        report
            .flags
            .iter()
            .any(|f| f == "robots.txt now disallows the feed"),
        "{:?}",
        report.flags
    );
    assert_eq!(
        board.status(),
        Status::Approved,
        "never paused by itself: the owner decides"
    );
    assert!(
        board
            .recheck_after
            .is_some_and(|at| at > today() + TimeDelta::days(100))
    );
    let run: (Option<String>, i32) =
        sqlx::query_as("SELECT error_kind, rechecked FROM sources.discovery_runs")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(run, (Some("list_unreachable".to_owned()), 1));
}

/// A board straight into the registry from the seed list (so its config is
/// never replaced by a check), with no config, its site at `site`.
async fn seeded_without_config(db: &PgPool, site: &MockServer, kind: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, config, terms, created_at, updated_at)
         VALUES ($1, 'feed-site', 'Feed Site', $2, '127.0.0.1', $3, 'low', 'candidate', 'seed',
                 '{\"fields\": {}}', '{}', now(), now())",
    )
    .bind(id)
    .bind(format!("{}/", site.uri()))
    .bind(kind)
    .execute(db)
    .await
    .unwrap();
    id
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_without_a_config_gets_one_proposed_from_its_rss_link(db: PgPool) {
    let site = a_board_site().await;
    let fake = Fake::ready();
    fake.signed_in_as("owner@example.com");
    let h = Harness::new(
        db,
        crate::SourcesConfig {
            board_owners: Some(vec!["owner@example.com".into()]),
            ..boards_config()
        },
        None,
        fake,
    )
    .await;
    let id = seeded_without_config(&h.db, &site, "json_api").await;
    let (_, before) = h.get(&format!("/api/v1/boards/{id}")).await;
    assert_eq!(before["blocked"], "no_endpoint", "{before}");

    check_one(&h.deps, &h.db, id).await.unwrap();
    let row = job_boards::get(&h.db, id).await.unwrap().unwrap();
    assert_eq!(
        row.config.0,
        serde_json::json!({ "fields": {} }),
        "a seed's config stays"
    );
    let report = row.report().unwrap();
    let proposal = report.proposal.clone().unwrap();
    assert_eq!(proposal.kind, Kind::Rss);
    assert!(
        proposal.config["endpoint"]
            .as_str()
            .unwrap()
            .ends_with("/feed.xml")
    );
    assert_eq!(proposal.config["fields"]["title"], "title");
    assert_eq!(report.sample.len(), 3);
    assert_eq!(requests_to(&site, "/feed.xml").await, 1);

    // "Use this config": the proposal, its kind with it; then approvable.
    let request = axum::http::Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/boards/{id}/config"))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({ "config": proposal.config, "kind": "rss" }).to_string(),
        ))
        .unwrap();
    let (status, detail) = h.send(request).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{detail}");
    assert_eq!(detail["board"]["kind"], "rss");
    assert!(detail["report"]["proposal"].is_null(), "used");
    assert_eq!(detail["approvable"], true, "{detail}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_with_postings_in_its_sitemap_gets_a_sitemap_config_proposed(db: PgPool) {
    let site = MockServer::start().await;
    let page =
        |body: String, kind: &str| ResponseTemplate::new(200).set_body_raw(body.into_bytes(), kind);
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(page(
            "User-agent: *\nDisallow: /admin/\n".into(),
            "text/plain",
        ))
        .mount(&site)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(page(
            "<html lang=\"en\"><body>Jobs.</body></html>".into(),
            "text/html",
        ))
        .mount(&site)
        .await;
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(page(
            format!(
                "<urlset><url><loc>{0}/about</loc></url><url><loc>{0}/jobs/1-engineer</loc></url>\
                 <url><loc>{0}/jobs/2-designer</loc></url></urlset>",
                site.uri()
            ),
            "application/xml",
        ))
        .mount(&site)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/jobs/\d-[a-z]+$"))
        .respond_with(page(
            r#"<html><script type="application/ld+json">{"@type": "JobPosting",
               "title": "Staff Engineer", "hiringOrganization": {"name": "Example Co"},
               "datePosted": "2026-10-26"}</script></html>"#
                .into(),
            "text/html",
        ))
        .mount(&site)
        .await;
    let h = Harness::new(db, boards_config(), None, Fake::ready()).await;
    let id = seeded_without_config(&h.db, &site, "jsonld_sitemap").await;
    check_one(&h.deps, &h.db, id).await.unwrap();
    let report = job_boards::get(&h.db, id)
        .await
        .unwrap()
        .unwrap()
        .report()
        .unwrap();
    let proposal = report.proposal.unwrap();
    assert_eq!(proposal.kind, Kind::JsonldSitemap);
    assert!(
        proposal.config["sitemap"]
            .as_str()
            .unwrap()
            .ends_with("/sitemap.xml")
    );
    assert_eq!(proposal.config["job_path"], "/jobs/");
    assert_eq!(report.sample.len(), 2);
    assert_eq!(report.sample[0].title, "Staff Engineer");
    // The probes were each asked once; the page that is no posting never.
    for probe in PROBES {
        assert!(requests_to(&site, probe).await <= 1, "{probe}");
    }
    assert_eq!(requests_to(&site, "/about").await, 0);
}
