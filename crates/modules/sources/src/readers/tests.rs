//! The readers against recorded answers of each board's API (trimmed, with
//! invented text), served by a fake board (wiremock).

use std::sync::Arc;

use sqlx::PgPool;
use url::Url;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use super::*;
use crate::{config::SourcesConfig, job::WorkMode, testkit};

const LEVER: &str = include_str!("fixtures/lever.json");
const ASHBY: &str = include_str!("fixtures/ashby.json");
const ARBEITNOW: &str = include_str!("fixtures/arbeitnow.json");
const HN_STORIES: &str = include_str!("fixtures/hn_stories.json");
const HN_COMMENTS: &str = include_str!("fixtures/hn_comments.json");

fn board(kind: Source, token: &str) -> Board {
    Board {
        kind,
        token: token.into(),
        company: "Sample Labs".into(),
        eu: false,
    }
}

fn reader(server: &MockServer, cache_secs: u64) -> Reader {
    let url: Url = server.uri().parse().unwrap();
    Reader::new(Arc::new(SourcesConfig {
        greenhouse_url: url.clone(),
        lever_url: url.clone(),
        lever_eu_url: url.clone(),
        ashby_url: url.clone(),
        arbeitnow_url: url.clone(),
        hn_url: url,
        cache_secs: Some(cache_secs),
        requests_per_second: Some(1_000),
        enabled: Some(vec!["greenhouse".into()]),
        ..SourcesConfig::default()
    }))
    .unwrap()
}

fn json(body: &'static str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "application/json")
}

#[sqlx::test(migrations = "../../../migrations")]
async fn greenhouse_lists_then_describes(db: PgPool) {
    let server = testkit::greenhouse_boards().await;
    let reader = reader(&server, 0);
    let mut jobs = reader.board(&db, &testkit::exampleco()).await.unwrap();
    assert_eq!(jobs.len(), 3);
    let first = &jobs[0];
    assert_eq!(first.source_ref, "exampleco:7001001");
    assert_eq!(first.title, "Senior Backend Engineer");
    assert_eq!(first.company, "Example Co");
    assert_eq!(first.location.as_deref(), Some("Remote - Europe"));
    assert_eq!(first.work_mode, Some(WorkMode::Remote));
    assert!(first.posted_at.is_some());
    assert!(first.description.is_empty(), "the list has none");
    assert_eq!(
        jobs[1].work_mode, None,
        "a city says nothing of the work mode"
    );

    reader.describe(&db, &mut jobs[0]).await.unwrap();
    let text = &jobs[0].description;
    assert!(text.contains("Example Co builds tools"), "{text}");
    assert!(!text.contains("not text"), "scripts are dropped: {text}");
    assert!(!text.contains('<'), "no markup: {text}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn lever_and_ashby_postings_are_read(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/postings/samplelabs"))
        .and(query_param("mode", "json"))
        .respond_with(json(LEVER))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/posting-api/job-board/samplelabs"))
        .and(query_param("includeCompensation", "true"))
        .respond_with(json(ASHBY))
        .mount(&server)
        .await;
    let reader = reader(&server, 0);

    let lever = reader
        .board(&db, &board(Source::Lever, "samplelabs"))
        .await
        .unwrap();
    assert_eq!(lever.len(), 2);
    assert_eq!(lever[0].source_ref, "0f3c2a1e-1111-4a6b-9c1d-2e3f4a5b6c7d");
    assert_eq!(lever[0].title, "Staff Software Engineer, Payments");
    assert_eq!(lever[0].work_mode, Some(WorkMode::Remote));
    assert_eq!(lever[0].company, "Sample Labs");
    assert!(
        lever[0]
            .salary_text
            .as_deref()
            .is_some_and(|s| s.contains("90000"))
    );
    assert!(!lever[0].description.is_empty());
    assert_eq!(lever[1].work_mode, Some(WorkMode::Hybrid));
    assert_eq!(lever[1].location.as_deref(), Some("Paris, France"));
    // Lever's `workplaceType` is a structured answer to "remote?".
    assert_eq!(lever[0].board_remote, Some(true));
    assert_eq!(lever[1].board_remote, Some(false));

    let ashby = reader
        .board(&db, &board(Source::Ashby, "samplelabs"))
        .await
        .unwrap();
    assert_eq!(ashby.len(), 2);
    assert_eq!(ashby[0].title, "Senior Platform Engineer");
    assert_eq!(ashby[0].work_mode, Some(WorkMode::Remote));
    assert_eq!(
        ashby[0].salary_text.as_deref(),
        Some("€80K – €100K • Offers Equity")
    );
    assert_eq!(ashby[1].work_mode, Some(WorkMode::Hybrid));
    assert!(ashby[1].posted_at.is_some());
    assert_eq!(ashby[0].board_remote, Some(true));
    assert_eq!(ashby[1].board_remote, Some(false));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn arbeitnow_and_hn_are_read(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/job-board-api"))
        .and(query_param("page", "1"))
        .respond_with(json(ARBEITNOW))
        .mount(&server)
        .await;
    // Page 2 is empty: the reading stops there.
    Mock::given(method("GET"))
        .and(path("/api/job-board-api"))
        .and(query_param("page", "2"))
        .respond_with(json(r#"{"data": [], "links": {"next": null}}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search_by_date"))
        .and(query_param("tags", "story,author_whoishiring"))
        .respond_with(json(HN_STORIES))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search_by_date"))
        .and(query_param("tags", "comment,story_49922569"))
        .respond_with(json(HN_COMMENTS))
        .mount(&server)
        .await;
    let reader = reader(&server, 0);

    let arbeitnow = reader.arbeitnow(&db).await;
    assert_eq!((arbeitnow.read, arbeitnow.failed), (2, 0));
    assert_eq!(arbeitnow.jobs.len(), 2);
    assert_eq!(
        arbeitnow.jobs[0].source_ref,
        "backend-engineer-berlin-100001"
    );
    assert_eq!(arbeitnow.jobs[0].work_mode, Some(WorkMode::Remote));
    assert_eq!(
        arbeitnow.jobs[0].board_remote, None,
        "Arbeitnow's remote flag is never a requirement's evidence"
    );
    assert_eq!(arbeitnow.jobs[1].work_mode, None, "not remote says nothing");

    let hn = reader.hn(&db).await;
    assert_eq!(hn.failed, 0);
    // The two top-level comments of the newest "Who is hiring?", not the reply.
    let refs: Vec<&str> = hn.jobs.iter().map(|j| j.source_ref.as_str()).collect();
    assert_eq!(refs, ["50200001", "50200002"]);
    assert!(
        hn.jobs[0]
            .title
            .starts_with("Example Co | Senior Backend Engineer")
    );
    assert!(hn.jobs[0].needs_reading());
    assert_eq!(
        hn.jobs[0].url,
        "https://news.ycombinator.com/item?id=50200001"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn answers_are_cached_and_failures_counted(db: PgPool) {
    let server = testkit::greenhouse_boards().await;
    Mock::given(method("GET"))
        .and(path("/v1/boards/gone/jobs"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let reader = reader(&server, 3_600);
    let boards = [testkit::exampleco(), board(Source::Greenhouse, "gone")];
    let first = reader.all(&db, &boards).await;
    let again = reader.all(&db, &boards).await;
    assert_eq!((first.read, first.failed, first.jobs.len()), (1, 1, 3));
    assert_eq!(again.jobs.len(), 3);
    let asked = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/boards/exampleco/jobs")
        .count();
    assert_eq!(asked, 1, "the second read came from the cache");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_answer_over_the_cap_is_refused(db: PgPool) {
    let server = MockServer::start().await;
    let huge = format!(r#"{{"jobs": [], "pad": "{}"}}"#, "x".repeat(MAX_BODY));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(huge))
        .mount(&server)
        .await;
    let err = reader(&server, 0)
        .board(&db, &testkit::exampleco())
        .await
        .unwrap_err();
    assert_eq!(err, FetchError::TooLarge);
}

#[test]
fn only_board_postings_are_links_the_server_reads() {
    let link = |raw: &str| posting_link(&raw.parse().unwrap());
    assert_eq!(
        link("https://boards.greenhouse.io/exampleco/jobs/4012345?gh_src=x"),
        Some((
            Source::Greenhouse,
            "exampleco".into(),
            "4012345".into(),
            false
        ))
    );
    assert_eq!(
        link("https://job-boards.greenhouse.io/exampleco/jobs/4012345"),
        Some((
            Source::Greenhouse,
            "exampleco".into(),
            "4012345".into(),
            false
        ))
    );
    assert_eq!(
        link("https://jobs.eu.lever.co/samplelabs/0F3C2A1E-1111-4A6B-9C1D-2E3F4A5B6C7D/apply"),
        Some((
            Source::Lever,
            "samplelabs".into(),
            "0f3c2a1e-1111-4a6b-9c1d-2e3f4a5b6c7d".into(),
            true
        ))
    );
    assert_eq!(
        link("https://jobs.ashbyhq.com/samplelabs/9b8a7c6d-1111-4e5f-8a9b-0c1d2e3f4a5b"),
        Some((
            Source::Ashby,
            "samplelabs".into(),
            "9b8a7c6d-1111-4e5f-8a9b-0c1d2e3f4a5b".into(),
            false
        ))
    );
    for other in [
        "https://www.linkedin.com/jobs/view/123",
        "https://careers.example.org/jobs/1",
        "https://boards.greenhouse.io.evil.example/exampleco/jobs/1",
        "https://boards.greenhouse.io/exampleco/jobs/not-a-number",
        "https://boards.greenhouse.io/%2e%2e/jobs/1",
        "ftp://jobs.lever.co/x/y",
    ] {
        assert_eq!(link(other), None, "{other}");
    }
}

#[test]
fn tokens_are_checked() {
    assert!(is_token("exampleco"));
    assert!(is_token("sample-labs.eu_2"));
    for bad in ["", "..", "a/b", "-x", "a..b", "x y"] {
        assert!(!is_token(bad), "{bad}");
    }
    assert!(!is_token(&"a".repeat(101)));
}
