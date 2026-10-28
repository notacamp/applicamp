//! The readers against recorded answers of each board's API (trimmed, with
//! invented text), served by a fake board (wiremock): the company boards,
//! HN, and the job boards of the registry through their seed config.

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
async fn hn_is_read(db: PgPool) {
    let server = MockServer::start().await;
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

fn board_reader(allow_private: bool) -> Reader {
    Reader::new(Arc::new(SourcesConfig {
        allow_private_hosts: Some(allow_private),
        ..testkit::boards_config()
    }))
    .unwrap()
}

async fn approved_board(db: &PgPool, slug: &str) -> crate::job_boards::BoardRow {
    crate::job_boards::approved(db)
        .await
        .unwrap()
        .into_iter()
        .find(|b| b.slug == slug)
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_first_job_boards_are_read_through_their_config(db: PgPool) {
    let server = testkit::fake_job_boards().await;
    testkit::seed_fake_boards(
        &db,
        &server,
        &["himalayas", "jobicy", "remoteok", "arbeitnow"],
    )
    .await;
    let reader = board_reader(true);
    let now = testkit::today();
    let read = |slug: &'static str| {
        let reader = reader.clone();
        let db = db.clone();
        async move {
            let row = approved_board(&db, slug).await;
            reader.job_board(&db, &row, now, now).await
        }
    };

    // Himalayas: two pages by cursor; its link to the job and its apply
    // link unchanged; its regions; its expiry (its terms drop expired jobs).
    let himalayas = read("himalayas").await;
    assert_eq!(himalayas.outcome, Outcome::Read, "{:?}", himalayas.error);
    assert_eq!(himalayas.jobs.len(), 4);
    let first = &himalayas.jobs[0];
    assert_eq!(first.source_name(), "himalayas");
    assert_eq!(first.title, "Senior Backend Engineer");
    assert_eq!(first.company, "Example Co");
    assert_eq!(
        first.url,
        "https://himalayas.app/companies/example-co/jobs/senior-backend-engineer"
    );
    assert_eq!(first.work_mode, Some(WorkMode::Remote));
    assert!(first.description.contains("invented software") && !first.description.contains('<'));
    let on = first.board.as_ref().unwrap();
    assert_eq!(
        on.apply_url.as_deref(),
        Some("https://jobs.example.com/apply/42?utm_source=api")
    );
    assert_eq!(on.links[0].name, "Himalayas");
    let pay = on.pay.clone().unwrap();
    assert_eq!(
        (pay.min, pay.max, pay.currency.as_deref()),
        (Some(120_000.0), Some(150_000.0), Some("USD"))
    );
    assert_eq!(
        himalayas.jobs[1].board.as_ref().unwrap().regions,
        ["United States"]
    );
    assert!(
        himalayas.jobs[2]
            .board
            .as_ref()
            .unwrap()
            .expires_at
            .is_some_and(|at| at < now)
    );
    assert_eq!(himalayas.jobs[3].title, "Founding Engineer", "page 2");
    assert_eq!(testkit::requests_to(&server, "/himalayas").await, 2);

    // Jobicy: regions as text, pay per year.
    let jobicy = read("jobicy").await;
    assert_eq!(jobicy.jobs.len(), 2);
    assert_eq!(jobicy.jobs[0].board.as_ref().unwrap().regions, ["Anywhere"]);
    assert_eq!(
        jobicy.jobs[1].board.as_ref().unwrap().regions,
        ["Germany", "Portugal", "Spain", "UK"]
    );
    assert_eq!(
        jobicy.jobs[0].board.as_ref().unwrap().apply_url.as_deref(),
        Some(jobicy.jobs[0].url.as_str())
    );

    // Remote OK: the legal notice is not a job; 0 is no salary.
    let remoteok = read("remoteok").await;
    assert_eq!(remoteok.jobs.len(), 2);
    assert_eq!(
        remoteok.jobs[0].board.as_ref().unwrap().links[0].name,
        "Remote OK"
    );
    assert_eq!(remoteok.jobs[1].board.as_ref().unwrap().pay, None);
    assert_eq!(
        remoteok.jobs[0]
            .board
            .as_ref()
            .unwrap()
            .pay
            .as_ref()
            .unwrap()
            .currency
            .as_deref(),
        Some("USD")
    );

    // Arbeitnow: remote jobs only, page 2 is the last.
    let arbeitnow = read("arbeitnow").await;
    assert_eq!(arbeitnow.jobs.len(), 1);
    assert_eq!(
        arbeitnow.jobs[0].source_ref,
        "senior-backend-engineer-example-co-100001"
    );
    assert_eq!(
        arbeitnow.jobs[0].board_remote, None,
        "never a requirement's evidence"
    );
    assert_eq!(testkit::requests_to(&server, "/arbeitnow").await, 2);

    // What each board did, and what it lists for its average salary.
    let row = approved_board(&db, "himalayas").await;
    assert_eq!(row.last_fetch_jobs, Some(4));
    assert_eq!(row.last_error, None);
    let listed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sources.board_jobs WHERE board_id = $1")
            .bind(row.id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(listed, 4);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_is_not_fetched_twice_within_its_polling_interval(db: PgPool) {
    let server = testkit::fake_job_boards().await;
    testkit::seed_fake_boards(&db, &server, &["remoteok"]).await;
    let reader = board_reader(true);
    let now = testkit::today();
    let row = approved_board(&db, "remoteok").await;
    let first = reader.job_board(&db, &row, now, now).await;
    assert_eq!(first.outcome, Outcome::Read);
    let later = now + chrono::TimeDelta::minutes(10);
    let row = approved_board(&db, "remoteok").await;
    let again = reader.job_board(&db, &row, later, later).await;
    assert_eq!(again.outcome, Outcome::SkippedPollLimit);
    assert_eq!(
        again.jobs.len(),
        first.jobs.len(),
        "its last answer is used"
    );
    assert_eq!(testkit::requests_to(&server, "/remoteok").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_that_asks_to_slow_down_is_left_alone(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    testkit::seed_fake_boards(&db, &server, &["jobicy"]).await;
    let reader = board_reader(true);
    let now = testkit::today();
    let row = approved_board(&db, "jobicy").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.outcome, Outcome::Error);
    assert_eq!(read.error.as_deref(), Some("rate_limited"));
    let row = approved_board(&db, "jobicy").await;
    assert_eq!(
        row.backoff_until,
        Some(now + chrono::TimeDelta::seconds(120))
    );
    let again = reader.job_board(&db, &row, now, now).await;
    assert_eq!(again.error.as_deref(), Some("backing_off"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_private_host_is_never_read(db: PgPool) {
    let server = testkit::fake_job_boards().await;
    testkit::seed_fake_boards(&db, &server, &["remoteok"]).await;
    let reader = board_reader(false);
    let now = testkit::today();
    let row = approved_board(&db, "remoteok").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.error.as_deref(), Some("refused_host"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_feed_board_is_read_by_element(db: PgPool) {
    let server = MockServer::start().await;
    let feed = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel>
        <item><title>Example Co: Senior Backend Engineer</title><region>Anywhere in the World</region>
        <link>https://board.example/jobs/1</link><guid>https://board.example/jobs/1</guid>
        <pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate><description><![CDATA[<p>Invented.</p>]]></description></item>
        </channel></rss>"#;
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(feed, "application/rss+xml"))
        .mount(&server)
        .await;
    let config = serde_json::json!({
        "endpoint": format!("{}/feed.xml", server.uri()),
        "remote_only": true,
        "title_split": { "separator": ": ", "company_first": true },
        "fields": { "ref": "guid", "title": "title", "url": "link", "regions": "region",
                    "description": "description", "posted_at": "pubDate" }
    });
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, config, terms, created_at, updated_at)
         VALUES ($1, 'example-feed', 'Example Feed', 'https://board.example', 'board.example',
                 'rss', 'high', 'approved', 'seed', $2, '{}', now(), now())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(sqlx::types::Json(&config))
    .execute(&db)
    .await
    .unwrap();
    let row = approved_board(&db, "example-feed").await;
    let now = testkit::today();
    let read = board_reader(true).job_board(&db, &row, now, now).await;
    assert_eq!(read.outcome, Outcome::Read, "{:?}", read.error);
    assert_eq!(read.jobs.len(), 1);
    assert_eq!(read.jobs[0].company, "Example Co");
    assert_eq!(read.jobs[0].title, "Senior Backend Engineer");
    assert_eq!(
        read.jobs[0].board.as_ref().unwrap().links[0].name,
        "Example Feed"
    );
}

const SWISSDEVJOBS: &str = include_str!("fixtures/boards/swissdevjobs.json");
const NET_EMPREGOS: &[u8] = include_bytes!("fixtures/boards/net_empregos.xml");

#[sqlx::test(migrations = "../../../migrations")]
async fn the_national_boards_are_read_through_their_seed_config(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/swissdevjobs/api/jobsLight"))
        .respond_with(json(SWISSDEVJOBS))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/net-empregos/rss.asp"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(NET_EMPREGOS, "application/xml; charset=iso-8859-1"),
        )
        .mount(&server)
        .await;
    testkit::seed_fake_boards(&db, &server, &["swissdevjobs", "net-empregos"]).await;
    let reader = board_reader(true);
    let now = testkit::today();

    // SwissDevJobs: every job in one answer, its page under /jobs/ on the
    // board, where it is worked, its pay in francs a year.
    let row = approved_board(&db, "swissdevjobs").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.outcome, Outcome::Read, "{:?}", read.error);
    assert_eq!(read.jobs.len(), 3);
    let first = &read.jobs[0];
    assert_eq!(first.title, "Senior Backend Engineer (Rust)");
    assert_eq!(first.company, "Example AG");
    assert_eq!(
        first.url,
        "https://swissdevjobs.ch/jobs/Example-AG-Senior-Backend-Engineer"
    );
    assert_eq!(first.location.as_deref(), Some("Zürich"));
    assert_eq!(first.work_mode, Some(WorkMode::Hybrid));
    assert_eq!(read.jobs[1].work_mode, Some(WorkMode::Remote));
    assert_eq!(read.jobs[2].work_mode, Some(WorkMode::Onsite));
    assert!(first.posted_at.is_some());
    let pay = first.board.as_ref().unwrap().pay.clone().unwrap();
    assert_eq!(
        (
            pay.min,
            pay.max,
            pay.currency.as_deref(),
            pay.period.as_deref()
        ),
        (Some(120_000.0), Some(140_000.0), Some("CHF"), Some("year"))
    );

    // Net-Empregos: an ISO-8859-1 feed, its company in `dc:creator`.
    let row = approved_board(&db, "net-empregos").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.outcome, Outcome::Read, "{:?}", read.error);
    assert_eq!(read.jobs.len(), 2);
    assert_eq!(read.jobs[1].title, "Técnico de Suporte Informático - Porto");
    assert_eq!(read.jobs[1].company, "Amostra Serviços S.A.");
    assert_eq!(
        read.jobs[0].url,
        "https://www.net-empregos.com/90000001/programador-backend-rust-lisboa/"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_keyed_board_is_read_only_with_its_key_and_the_key_is_kept_nowhere(db: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/itjobs-pt/job/list.json"))
        .and(query_param("api_key", "test-key-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total": 1,
            "results": [{
                "id": 4242, "title": "Backend Developer", "company": { "name": "Exemplo Lda" },
                "body": "<p>Invented.</p>", "publishedAt": "2026-10-27 10:00:00",
                "locations": [{ "name": "Lisboa" }]
            }]
        })))
        .mount(&server)
        .await;
    testkit::seed_fake_boards(&db, &server, &["itjobs-pt"]).await;
    let secret = applicamp_kernel::crypto::SecretKey::from_bytes([3; 32]);
    let keys = Arc::new(crate::board_keys::cipher(&secret));
    let reader = Reader::new(Arc::new(SourcesConfig {
        cache_secs: Some(3_600),
        ..testkit::boards_config()
    }))
    .unwrap()
    .with_keys(Arc::clone(&keys));
    let now = testkit::today();

    // No key: never asked.
    let row = approved_board(&db, "itjobs-pt").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.error.as_deref(), Some("needs_key"));
    assert!(server.received_requests().await.unwrap().is_empty());

    // With its key: read, the key in the request only.
    let values = [("api_key".to_owned(), "test-key-1".to_owned())].into();
    let sealed = crate::board_keys::seal(&keys, row.id, &values);
    let host = Url::parse(&server.uri())
        .unwrap()
        .host_str()
        .unwrap()
        .to_owned();
    sqlx::query("UPDATE sources.job_boards SET api_key = $2, api_key_host = $3 WHERE id = $1")
        .bind(row.id)
        .bind(sealed)
        .bind(host)
        .execute(&db)
        .await
        .unwrap();
    let row = approved_board(&db, "itjobs-pt").await;
    let read = reader.job_board(&db, &row, now, now).await;
    assert_eq!(read.outcome, Outcome::Read, "{:?}", read.error);
    assert_eq!(read.jobs.len(), 1);
    assert_eq!(read.jobs[0].url, "https://www.itjobs.pt/oferta/4242");
    assert_eq!(read.jobs[0].company, "Exemplo Lda");
    let cached: Vec<String> = sqlx::query_scalar("SELECT url FROM sources.cache")
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(!cached.is_empty(), "the answer is cached");
    assert!(
        cached.iter().all(|url| !url.contains("test-key-1")),
        "{cached:?}"
    );

    // Another secret cannot open it: not read.
    let other = Reader::new(Arc::new(testkit::boards_config()))
        .unwrap()
        .with_keys(Arc::new(crate::board_keys::cipher(
            &applicamp_kernel::crypto::SecretKey::from_bytes([4; 32]),
        )));
    let read = other.job_board(&db, &row, now, now).await;
    assert_eq!(read.error.as_deref(), Some("needs_key"));
}

fn queries(requests: &[wiremock::Request], prefix: &str, param: &str) -> Vec<Option<String>> {
    requests
        .iter()
        .filter(|r| r.url.path().starts_with(prefix))
        .map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == param)
                .map(|(_, v)| v.into_owned())
        })
        .collect()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn boards_that_search_by_skill_are_asked_for_the_top_three_skills(db: PgPool) {
    let server = testkit::fake_job_boards().await;
    testkit::seed_fake_boards(
        &db,
        &server,
        &["himalayas", "jobicy", "remoteok", "arbeitnow"],
    )
    .await;
    let reader = board_reader(true);
    let now = testkit::today();
    let skills: Vec<String> = ["Ruby on Rails", "React", "PostgreSQL", "Rust"]
        .map(String::from)
        .into();
    for slug in ["himalayas", "jobicy", "remoteok", "arbeitnow"] {
        let row = approved_board(&db, slug).await;
        let plain = reader.job_board(&db, &row, now, now).await;
        let stacked = reader.job_board_for(&db, &row, now, now, &skills).await;
        assert_eq!(
            stacked.outcome,
            Outcome::Read,
            "{slug}: {:?}",
            stacked.error
        );
        // The same fake answers every query: merged without duplicates.
        assert_eq!(stacked.jobs.len(), plain.jobs.len(), "{slug}");
    }
    let seen = server.received_requests().await.unwrap();
    // Remote OK: its plain read, then its tags (its own spellings).
    let tags = queries(&seen, "/remoteok", "tag");
    assert_eq!(
        tags,
        [
            None,
            Some("ruby".into()),
            Some("react".into()),
            Some("postgres".into())
        ]
    );
    // Jobicy: a keyword search; the plain read asks for `engineer`.
    let tags = queries(&seen, "/jobicy", "tag");
    assert_eq!(
        tags,
        [
            Some("engineer".into()),
            Some("rails".into()),
            Some("react".into()),
            Some("postgresql".into())
        ]
    );
    // Himalayas: its search address, once per skill, one page each.
    let search: Vec<_> = queries(&seen, "/himalayas/jobs/api/search", "q");
    assert_eq!(
        search,
        [
            Some("ruby on rails".into()),
            Some("react".into()),
            Some("postgresql".into())
        ]
    );
    // Arbeitnow cannot search by skill: its two pages, as before.
    assert_eq!(testkit::requests_to(&server, "/arbeitnow").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_board_is_read_as_before_without_skills(db: PgPool) {
    let server = testkit::fake_job_boards().await;
    testkit::seed_fake_boards(&db, &server, &["remoteok"]).await;
    let reader = board_reader(true);
    let now = testkit::today();
    let row = approved_board(&db, "remoteok").await;
    let read = reader.job_board_for(&db, &row, now, now, &[]).await;
    assert_eq!(read.outcome, Outcome::Read);
    assert_eq!(testkit::requests_to(&server, "/remoteok").await, 1);
}

#[test]
fn a_board_with_a_daily_limit_spreads_its_addresses_over_the_day() {
    let hour = Duration::from_secs(3_600);
    // Jobicy: 24 fetches a day, the plain read and three queries.
    assert_eq!(stack_ttl(hour, 24, 3), Duration::from_secs(4 * 3_600));
    // A roomy limit keeps the polling interval.
    assert_eq!(stack_ttl(hour, 1_000, 3), hour);
}
