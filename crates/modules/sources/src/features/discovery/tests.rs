use chrono::{TimeZone as _, Utc};
use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
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
