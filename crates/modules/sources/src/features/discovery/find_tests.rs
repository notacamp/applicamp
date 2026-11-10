//! The check finds a site's list of jobs in more places and knows a posting
//! by its link (#108): the feed paths, platforms, the Jobs page, sitemaps,
//! the terms page, the request budget and the re-check queue.

use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex, query_param},
};

use super::*;
use crate::job_boards::WaitingCode;
use crate::testkit::{Fake, Harness, boards_config, requests_to, today};

/// Requests `server` got for exactly `target` (a path, with its query if it
/// has one).
pub(super) async fn asked(server: &MockServer, target: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| {
            let got = match r.url.query() {
                Some(query) => format!("{}?{query}", r.url.path()),
                None => r.url.path().to_owned(),
            };
            got == target
        })
        .count()
}

async fn mount(server: &MockServer, route: &str, body: &str, kind: &str) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), kind))
        .mount(server)
        .await;
}

async fn robots(server: &MockServer, text: &str) {
    mount(server, "/robots.txt", text, "text/plain").await;
}

/// A feed of `n` items titled `<role> at Company <n>`, each linking to its
/// own page under `/jobs/`, dated when `date` is given.
fn feed(server: &MockServer, n: usize, date: Option<&str>) -> String {
    let items: String = (1..=n)
        .map(|i| {
            format!(
                "<item><title>Rust Engineer {i} at Company {i}</title><link>{}/jobs/{i}-rust</link>\
                 <guid>{i}</guid>{}</item>",
                server.uri(),
                date.map_or_else(String::new, |d| format!("<pubDate>{d}</pubDate>"))
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Jobs</title>{items}</channel></rss>"
    )
}

/// A candidate straight in the registry, its site at `server`, with no
/// config; each `n` is another site.
async fn candidate(db: &PgPool, server: &MockServer, n: usize, origin: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, config, terms, created_at, updated_at)
         VALUES ($1, $2, 'Example Site', $3, $4, 'json_api', 'low', 'candidate', $5,
                 '{\"fields\": {}}', '{}', now(), now())",
    )
    .bind(id)
    .bind(format!("site-{n}"))
    .bind(format!("{}/", server.uri()))
    .bind(format!("site{n}.example"))
    .bind(origin)
    .execute(db)
    .await
    .unwrap();
    id
}

async fn harness(db: PgPool) -> Harness {
    Harness::new(db, boards_config(), None, Fake::ready()).await
}

async fn checked(h: &Harness, id: Uuid) -> BoardRow {
    check_one(&h.deps, &h.db, id).await.unwrap();
    job_boards::get(&h.db, id).await.unwrap().unwrap()
}

const HOME: &str = "<html lang=\"en\"><body>Jobs for people.</body></html>";

#[sqlx::test(migrations = "../../../migrations")]
async fn titles_give_the_company_and_the_last_modified_date_keeps_it_alive(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    Mock::given(method("GET"))
        .and(path("/feed"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Last-Modified", "Sat, 24 Oct 2026 08:00:00 GMT")
                .set_body_raw(feed(&site, 6, None).into_bytes(), "application/rss+xml"),
        )
        .mount(&site)
        .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert_eq!(
        report.found_url.as_deref().map(|u| u.ends_with("/feed")),
        Some(true)
    );
    assert_eq!(
        report.sample[0].company, "Company 1",
        "{:?}",
        report.sample[0]
    );
    assert_eq!(report.sample[0].title, "Rust Engineer 1");
    let config = row.config().unwrap();
    let split = config.title_split.unwrap();
    assert_eq!((split.separator.as_str(), split.last), (" at ", true));
    assert_eq!(
        config.fields.posted_at.as_deref(),
        Some("pubDate"),
        "dates are mapped by default"
    );
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        report.waiting_because
    );
    assert!(report.waiting_because.is_none());
    assert_eq!(report.rules, RULES);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_feed_with_no_dates_and_no_recent_change_waits_and_says_why(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    Mock::given(method("GET"))
        .and(path("/feed"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Last-Modified", "Mon, 01 Jun 2026 08:00:00 GMT")
                .set_body_raw(feed(&site, 6, None).into_bytes(), "application/rss+xml"),
        )
        .mount(&site)
        .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(row.status(), Status::Candidate);
    let because = row.report().unwrap().waiting_because;
    assert_eq!(because.unwrap().code, WaitingCode::Undated);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_feeds_last_build_date_counts_when_items_have_no_date(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let body = feed(&site, 6, None).replace(
        "<title>Jobs</title>",
        "<title>Jobs</title><lastBuildDate>Sat, 24 Oct 2026 08:00:00 +0000</lastBuildDate>",
    );
    mount(&site, "/feed", &body, "application/rss+xml").await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        row.report().unwrap().waiting_because
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_item_with_only_a_link_to_its_own_job_page_is_a_posting(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    // No company, no pattern in the titles: the link to the job's own page.
    let items: String = [
        "Designer",
        "Analyst",
        "Support Lead",
        "Writer",
        "Recruiter",
        "Tester",
    ]
    .iter()
    .enumerate()
    .map(|(i, title)| {
        format!(
            "<item><title>{title}</title><link>{}/job/{i}-x</link>\
                 <pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate></item>",
            site.uri()
        )
    })
    .collect();
    let body = format!("<rss><channel>{items}</channel></rss>");
    mount(&site, "/feed", &body, "application/rss+xml").await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        row.report().unwrap().waiting_because
    );

    // A blog's posts are not postings.
    let blog = MockServer::start().await;
    robots(&blog, "User-agent: *\nDisallow: /private\n").await;
    mount(&blog, "/", HOME, "text/html").await;
    let posts: String = (0..6)
        .map(|i| {
            format!(
                "<item><title>Post {i}</title><link>{}/blog/2026/post-{i}</link>\
                 <pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate></item>",
                blog.uri()
            )
        })
        .collect();
    mount(
        &blog,
        "/feed",
        &format!("<rss><channel>{posts}</channel></rss>"),
        "application/rss+xml",
    )
    .await;
    let id = candidate(&h.db, &blog, 2, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(row.status(), Status::Candidate);
    assert_eq!(
        row.report().unwrap().waiting_because.unwrap().code,
        WaitingCode::NotAJobBoard
    );
}

/// A query parameter a mock requires.
type Query = Option<(&'static str, &'static str)>;

/// A site that serves a feed of 6 items at `route` only (with `query`).
async fn feed_only_at(route: &str, query: Query) -> MockServer {
    let site = MockServer::start().await;
    // The feed first: a mock mounted earlier answers before a later one.
    let body = feed(&site, 6, Some("Sat, 24 Oct 2026 08:00:00 +0000"));
    let mut mock = Mock::given(method("GET")).and(path(route));
    if let Some((key, value)) = query {
        mock = mock.and(query_param(key, value));
    }
    mock.respond_with(
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/rss+xml"),
    )
    .mount(&site)
    .await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    site
}

#[sqlx::test(migrations = "../../../migrations")]
async fn each_feed_path_is_probed_once(db: PgPool) {
    let h = harness(db).await;
    let paths: [(&str, Query, &str); 8] = [
        ("/jobs.rss", None, "/jobs.rss"),
        ("/jobs/feed", None, "/jobs/feed"),
        ("/feed/jobs", None, "/feed/jobs"),
        ("/rss.xml", None, "/rss.xml"),
        ("/feed.xml", None, "/feed.xml"),
        ("/jobs.xml", None, "/jobs.xml"),
        ("/", Some(("feed", "job_feed")), "/?feed=job_feed"),
        (
            "/feed/",
            Some(("post_type", "job_listing")),
            "/feed/?post_type=job_listing",
        ),
    ];
    for (n, (route, query, expect)) in paths.into_iter().enumerate() {
        let site = feed_only_at(route, query).await;
        let id = candidate(&h.db, &site, n + 1, "seed").await;
        let row = checked(&h, id).await;
        let report = row.report().unwrap();
        let found = report.found_url.clone().unwrap_or_default();
        assert!(
            found.ends_with(expect),
            "{expect}: {found} / {:?}",
            report.waiting_because
        );
        assert_eq!(report.sample.len(), 6, "{expect}");
        assert_eq!(asked(&site, expect).await, 1, "{expect} once");
        assert_eq!(asked(&site, "/robots.txt").await, 1);
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_json_list_at_jobs_json_is_probed(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let jobs: Vec<_> = (1..=6)
        .map(|i| {
            serde_json::json!({ "id": i, "title": format!("Engineer {i}"), "company": format!("Company {i}"),
                "url": format!("{}/jobs/{i}", site.uri()), "datePosted": "2026-10-24" })
        })
        .collect();
    mount(
        &site,
        "/jobs.json",
        &serde_json::to_string(&jobs).unwrap(),
        "application/json",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(row.kind(), Some(Kind::JsonApi));
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        row.report().unwrap().waiting_because
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_path_the_sites_rules_disallow_is_never_asked(db: PgPool) {
    let site = MockServer::start().await;
    // The guessed feed paths are off limits.
    robots(
        &site,
        "User-agent: *\nDisallow: /jobs.rss\nDisallow: /feed.xml\n",
    )
    .await;
    mount(&site, "/", HOME, "text/html").await;
    mount(
        &site,
        "/feed.xml",
        &feed(&site, 6, Some("Sat, 24 Oct 2026 08:00:00 +0000")),
        "application/rss+xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "seed").await;
    let row = checked(&h, id).await;
    assert_eq!(asked(&site, "/jobs.rss").await, 0);
    assert_eq!(asked(&site, "/feed.xml").await, 0);
    let report = row.report().unwrap();
    assert!(report.found_url.is_none());
    assert_eq!(report.waiting_because.unwrap().code, WaitingCode::NoJobList);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_jobs_page_and_its_feed_link_are_followed(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(
        &site,
        "/",
        "<html><a href=\"/about\">About</a><a href=\"/jobs\">Jobs</a></html>",
        "text/html",
    )
    .await;
    mount(
        &site,
        "/jobs",
        "<html><head><link rel=\"alternate\" type=\"application/rss+xml\" href=\"/latest-openings.rss\"></head>\
         <body><a href=\"/legal-notice\">Terms of use</a></body></html>",
        "text/html",
    )
    .await;
    mount(
        &site,
        "/latest-openings.rss",
        &feed(&site, 6, Some("Sat, 24 Oct 2026 08:00:00 +0000")),
        "application/rss+xml",
    )
    .await;
    let terms = format!(
        "<html><title>Terms of use</title><body><p>{}You may not scrape this site with automated tools.</p></body></html>",
        "These are the rules for using this site and its jobs. ".repeat(10)
    );
    mount(&site, "/legal-notice", &terms, "text/html").await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "seed").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(report.found_url.unwrap().ends_with("/latest-openings.rss"));
    // The terms link on the Jobs page is read, and no usual path is guessed.
    assert!(report.terms_url.unwrap().ends_with("/legal-notice"));
    assert_eq!(report.terms_sentences.len(), 1);
    for guess in finder::TERMS_PATHS {
        assert_eq!(asked(&site, guess).await, 0, "{guess}");
    }
    assert_eq!(asked(&site, "/jobs").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_terms_page_is_found_at_a_usual_path_but_not_a_catch_all_page(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let terms = format!(
        "<html><title>Terms of Service</title><body><p>{}Automated access is not allowed.</p></body></html>",
        "These are the rules for using this site. ".repeat(12)
    );
    mount(&site, "/terms-of-service", &terms, "text/html").await;
    mount(
        &site,
        "/feed.xml",
        &feed(&site, 6, Some("Sat, 24 Oct 2026 08:00:00 +0000")),
        "application/rss+xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "seed").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(report.terms_url.unwrap().ends_with("/terms-of-service"));
    assert_eq!(asked(&site, "/terms").await, 1);
    assert_eq!(asked(&site, "/terms-of-service").await, 1);
    assert_eq!(asked(&site, "/tos").await, 0, "stops at the first");

    // A site that answers every path with its front page has no terms page.
    let catch_all = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/.*$"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                format!(
                    "<html><title>Find jobs</title><p>{}</p></html>",
                    "Many jobs here. ".repeat(40)
                )
                .into_bytes(),
                "text/html",
            ),
        )
        .mount(&catch_all)
        .await;
    let id = candidate(&h.db, &catch_all, 2, "seed").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(report.terms_url.is_none());
    assert!(report.flags.iter().any(|f| f == "no terms page found"));
}

const JOB_PAGE: &str = r#"<html><script type="application/ld+json">{"@type": "JobPosting",
    "title": "Engineer N", "hiringOrganization": {"name": "Company N"}, "datePosted": "2026-10-24"}</script></html>"#;

/// A site shaped like a Niceboard board: robots.txt (with `rules`) lists a
/// sitemap index, whose jobs sitemap lists `jobs` pages under `/<dir>/`.
async fn index_site(jobs: usize, markup: &str, dir: &str, rules: &str) -> MockServer {
    let site = MockServer::start().await;
    robots(
        &site,
        &format!(
            "User-agent: *\nDisallow: /api/\n{rules}Sitemap: {}/sitemap.xml\n",
            site.uri()
        ),
    )
    .await;
    mount(
        &site,
        "/",
        &format!("<html lang=\"en\"><body>{markup}Jobs.</body></html>"),
        "text/html",
    )
    .await;
    let index = format!(
        "<sitemapindex><sitemap><loc>{0}/sitemap-base.xml</loc></sitemap>\
         <sitemap><loc>{0}/sitemap-jobs.xml</loc></sitemap>\
         <sitemap><loc>{0}/sitemap-companies.xml</loc></sitemap></sitemapindex>",
        site.uri()
    );
    mount(&site, "/sitemap.xml", &index, "application/xml").await;
    let urls: String = (1..=jobs)
        .map(|i| format!("<url><loc>{}/{dir}/{i}-engineer</loc></url>", site.uri()))
        .collect();
    mount(
        &site,
        "/sitemap-jobs.xml",
        &format!("<urlset>{urls}</urlset>"),
        "application/xml",
    )
    .await;
    Mock::given(method("GET"))
        .and(path_regex(format!(r"^/{dir}/\d+-engineer$")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(JOB_PAGE.as_bytes().to_vec(), "text/html"),
        )
        .mount(&site)
        .await;
    site
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_sitemap_index_is_followed_down_to_the_jobs(db: PgPool) {
    let site = index_site(9, "", "job", "").await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert_eq!(report.found_kind, Some(Kind::JsonldSitemap));
    assert!(
        report
            .found_url
            .clone()
            .unwrap()
            .ends_with("/sitemap-jobs.xml")
    );
    let config = row.config().unwrap();
    assert!(
        config
            .sitemap
            .unwrap()
            .path()
            .ends_with("/sitemap-jobs.xml")
    );
    assert_eq!(config.job_path.as_deref(), Some("/job/"));
    assert_eq!(report.sample.len(), SITEMAP_SAMPLE);
    // Only the sitemap of jobs was followed; five pages were read.
    assert_eq!(asked(&site, "/sitemap.xml").await, 1);
    assert_eq!(asked(&site, "/sitemap-jobs.xml").await, 1);
    assert_eq!(asked(&site, "/sitemap-base.xml").await, 0);
    assert_eq!(asked(&site, "/sitemap-companies.xml").await, 0);
    assert_eq!(requests_to(&site, "/job/").await, SITEMAP_SAMPLE);
    // No guessed feed path was needed.
    for probe in PROBES {
        assert_eq!(asked(&site, probe).await, 0, "{probe}");
    }
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        report.waiting_because
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn niceboard_is_recognised_and_read_through_its_sitemap(db: PgPool) {
    let site = index_site(
        7,
        "<img src=\"https://niceboard.co/boards/example/assets/bg.png\">",
        "job",
        "",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(row.config().unwrap().job_path.as_deref(), Some("/job/"));
    let (_, page) = h.get("/api/v1/boards/scans").await;
    let note = page["runs"][0]["lines"][0]["note"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(note.contains("(Niceboard)"), "{note}");
}

const WPJM_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"
 xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/">
<channel><title>Jobs Archive | Example</title><lastBuildDate>Sat, 24 Oct 2026 08:00:00 +0000</lastBuildDate>
<item><title>Product Manager</title><link>SITE/job/product-manager/</link>
<dc:creator><![CDATA[Acme Labs<br>⚲&nbsp;Germany]]></dc:creator><pubDate>Sat, 24 Oct 2026 08:00:00 +0000</pubDate>
<description><![CDATA[Invented text.]]></description></item>
<item><title>Designer</title><link>SITE/job/designer/</link><dc:creator><![CDATA[Globex<br>⚲&nbsp;Worldwide]]></dc:creator>
<pubDate>Sat, 24 Oct 2026 07:00:00 +0000</pubDate></item>
<item><title>Analyst</title><link>SITE/job/analyst/</link><dc:creator><![CDATA[Initech<br>⚲&nbsp;Canada]]></dc:creator>
<pubDate>Sat, 24 Oct 2026 06:00:00 +0000</pubDate></item>
<item><title>Writer</title><link>SITE/job/writer/</link><dc:creator><![CDATA[Umbrella<br>⚲&nbsp;France]]></dc:creator>
<pubDate>Sat, 24 Oct 2026 05:00:00 +0000</pubDate></item>
<item><title>Tester</title><link>SITE/job/tester/</link><dc:creator><![CDATA[Hooli<br>⚲&nbsp;Spain]]></dc:creator>
<pubDate>Sat, 24 Oct 2026 04:00:00 +0000</pubDate></item>
</channel></rss>"#;

#[sqlx::test(migrations = "../../../migrations")]
async fn wp_job_manager_is_read_through_its_feed_without_a_query(db: PgPool) {
    let site = MockServer::start().await;
    // Like many WordPress sites: nothing with a query string.
    robots(&site, "User-agent: *\nDisallow: /*?\n").await;
    mount(
        &site,
        "/",
        "<html lang=\"en\"><link rel=\"stylesheet\" href=\"/wp-content/plugins/wp-job-manager/assets/css/frontend.css\"></html>",
        "text/html",
    )
    .await;
    mount(
        &site,
        "/feed/job_feed/",
        &WPJM_FEED.replace("SITE", &site.uri()),
        "application/rss+xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(
        report
            .found_url
            .clone()
            .unwrap()
            .ends_with("/feed/job_feed/")
    );
    assert_eq!(
        report.sample[0].company, "Acme Labs",
        "the name before the line break"
    );
    assert_eq!(report.sample[0].location.as_deref(), Some("Germany"));
    assert_eq!(asked(&site, "/?feed=job_feed").await, 0);
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        report.waiting_because
    );
}

const BOARDLY_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<jobs>
<job><guid isPermaLink="false">101-it-support-ab12</guid><id>101</id><url>SITE/jobs/it-support-ab12</url>
<application_link>https://careers.example.com/apply/101</application_link><title>IT Support Analyst</title>
<location>Springfield, United States</location><published_at>2026-10-24T00:00:00-06:00</published_at>
<plain_text_description>Invented text.</plain_text_description><company_name>Example Motorsport Ltd</company_name>
<salary_minimum/><salary_maximum/></job>
<job><url>SITE/jobs/designer-cd34</url><application_link>https://careers.example.com/apply/102</application_link>
<title>Designer</title><published_at>2026-10-23T00:00:00-06:00</published_at><company_name>Globex</company_name></job>
<job><url>SITE/jobs/analyst-ef56</url><application_link>https://careers.example.com/apply/103</application_link>
<title>Analyst</title><published_at>2026-10-23T00:00:00-06:00</published_at><company_name>Initech</company_name></job>
<job><url>SITE/jobs/writer-aa11</url><application_link>https://careers.example.com/apply/104</application_link>
<title>Writer</title><published_at>2026-10-22T00:00:00-06:00</published_at><company_name>Umbrella</company_name></job>
<job><url>SITE/jobs/tester-bb22</url><application_link>https://careers.example.com/apply/105</application_link>
<title>Tester</title><published_at>2026-10-22T00:00:00-06:00</published_at><company_name>Hooli</company_name></job>
</jobs>"#;

#[sqlx::test(migrations = "../../../migrations")]
async fn job_boardly_is_read_through_its_xml_feed(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nAllow: /\n").await;
    mount(
        &site,
        "/",
        "<html><link href=\"https://www.jobboardly.com/assets/board-67e33a7c.css\" rel=\"stylesheet\"></html>",
        "text/html",
    )
    .await;
    mount(
        &site,
        "/jobs.xml",
        &BOARDLY_FEED.replace("SITE", &site.uri()),
        "application/xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let config = row.config().unwrap();
    assert_eq!(config.fields.company.as_deref(), Some("company_name"));
    assert_eq!(config.fields.url.as_deref(), Some("url"));
    assert_eq!(config.fields.apply_url.as_deref(), Some("application_link"));
    assert_eq!(config.fields.posted_at.as_deref(), Some("published_at"));
    let report = row.report().unwrap();
    assert_eq!(report.sample[0].company, "Example Motorsport Ltd");
    assert_eq!(report.sample[0].title, "IT Support Analyst");
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        report.waiting_because
    );
    // Its guessed paths came after the platform's address, so none was asked.
    assert_eq!(asked(&site, "/jobs.rss").await, 0);
}

const JBOARD_FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>All Jobs</title>
<item><title>Welder at Example Energy</title><company>Example Energy</company><description>Invented.</description><location>Springfield, ON</location><pubDate>2026-10-24 08:00:00</pubDate><guid>1</guid><link>SITE/jobs/1-welder</link></item>
<item><title>Planner at Globex</title><company>Globex</company><location>Toronto, ON</location><pubDate>2026-10-24 07:00:00</pubDate><guid>2</guid><link>SITE/jobs/2-planner</link></item>
<item><title>Clerk at Initech</title><company>Initech</company><location>Ottawa, ON</location><pubDate>2026-10-24 06:00:00</pubDate><guid>3</guid><link>SITE/jobs/3-clerk</link></item>
<item><title>Driver at Umbrella</title><company>Umbrella</company><location>Calgary, AB</location><pubDate>2026-10-24 05:00:00</pubDate><guid>4</guid><link>SITE/jobs/4-driver</link></item>
<item><title>Welder at Hooli</title><company>Hooli</company><location>Halifax, NS</location><pubDate>2026-10-24 04:00:00</pubDate><guid>5</guid><link>SITE/jobs/5-welder</link></item>
</channel></rss>"#;

#[sqlx::test(migrations = "../../../migrations")]
async fn jboard_is_read_through_its_feed_unless_its_rules_disallow_it(db: PgPool) {
    let markup = "<html><img src=\"https://jboard-tenant.s3.us-west-1.amazonaws.com/employers/x.png\"></html>";
    let h = harness(db).await;

    let open = MockServer::start().await;
    robots(&open, "User-agent: *\nAllow: /\n").await;
    mount(&open, "/", markup, "text/html").await;
    mount(
        &open,
        "/rss/jobs",
        &JBOARD_FEED.replace("SITE", &open.uri()),
        "application/rss+xml",
    )
    .await;
    let id = candidate(&h.db, &open, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(report.found_url.unwrap().ends_with("/rss/jobs"));
    assert_eq!(report.sample[0].company, "Example Energy");
    assert_eq!(
        row.status(),
        Status::Approved,
        "{:?}",
        report.waiting_because
    );

    // Most JBoard sites disallow /rss/: it is not asked, the sitemap is read.
    let closed = index_site(
        6,
        "<img src=\"https://jboard-tenant.s3.us-west-1.amazonaws.com/x.png\">",
        "jobs",
        "Disallow: /rss/\n",
    )
    .await;
    let id = candidate(&h.db, &closed, 2, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(asked(&closed, "/rss/jobs").await, 0);
    assert_eq!(row.report().unwrap().found_kind, Some(Kind::JsonldSitemap));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_whose_jobs_all_apply_on_one_hiring_system_is_a_company_page(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let jobs: Vec<_> = (1..=6)
        .map(|i| {
            serde_json::json!({ "id": i, "title": format!("Engineer {i}"), "company": "Acme Labs",
                "url": format!("{}/jobs/{i}", site.uri()), "datePosted": "2026-10-24",
                "applyUrl": format!("https://jobs.lever.co/acme-labs/0000-{i}") })
        })
        .collect();
    mount(
        &site,
        "/api",
        &serde_json::to_string(&jobs).unwrap(),
        "application/json",
    )
    .await;
    let h = harness(db).await;
    crate::company_pages::ensure_systems(&h.db, today())
        .await
        .unwrap();
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    assert_eq!(
        row.status(),
        Status::Candidate,
        "one company, not a job site"
    );
    let because = row.report().unwrap().waiting_because;
    let because = because.unwrap();
    assert_eq!(because.code, WaitingCode::JobsOnHiringSystem);
    assert_eq!(because.detail.as_deref(), Some("Lever"));
    let pages: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sources.job_boards WHERE kind = 'company_page' AND company_token = 'acme-labs'",
    )
    .fetch_one(&h.db)
    .await
    .unwrap();
    assert_eq!(pages, 1, "registered through the company pages' function");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_request_budget_bounds_a_check(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let settings = crate::SourcesConfig {
        discovery_max_requests: Some(7),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let total = site.received_requests().await.unwrap().len();
    // robots.txt and the homepage, then 7 requests at most.
    assert_eq!(total, 2 + 7);
    let because = row.report().unwrap().waiting_because;
    let because = because.unwrap();
    assert_eq!(because.code, WaitingCode::RequestLimit);
    assert_eq!(because.detail.as_deref(), Some("7"));
    assert_eq!(crate::SourcesConfig::default().discovery_max_requests(), 25);
    assert_eq!(
        crate::SourcesConfig {
            discovery_max_requests: Some(1),
            ..boards_config()
        }
        .discovery_max_requests(),
        5,
        "never fewer than 5"
    );
}

fn job(
    title: &str,
    company: &str,
    url: &str,
    at: Option<chrono::DateTime<Utc>>,
) -> crate::job::Job {
    crate::job::Job {
        source: crate::job::Source::Board,
        source_ref: url.to_owned(),
        title: title.to_owned(),
        company: company.to_owned(),
        location: None,
        work_mode: None,
        board_remote: None,
        url: url.to_owned(),
        posted_at: at,
        salary_text: None,
        description: String::new(),
        board: None,
    }
}

#[test]
fn a_site_is_alive_by_a_dated_job_or_by_its_own_date_when_no_job_has_one() {
    let now = today();
    let hosts = vec!["board.example".to_owned()];
    let recent = now - TimeDelta::days(3);
    let old = now - TimeDelta::days(90);
    let sample = |dates: [Option<chrono::DateTime<Utc>>; 5]| -> Vec<crate::job::Job> {
        dates
            .into_iter()
            .enumerate()
            .map(|(i, at)| {
                job(
                    "Engineer",
                    &format!("Co {i}"),
                    "https://board.example/job/1",
                    at,
                )
            })
            .collect()
    };
    // A dated job, however the feed is dated.
    assert!(is_a_live_job_board(
        &sample([Some(recent), None, None, None, None]),
        &hosts,
        now,
        None
    ));
    assert!(
        !is_a_live_job_board(&sample([Some(old); 5]), &hosts, now, Some(recent)),
        "dated jobs speak for themselves"
    );
    // No job has a date: the feed's own date counts, within 30 days.
    assert!(is_a_live_job_board(
        &sample([None; 5]),
        &hosts,
        now,
        Some(recent)
    ));
    assert!(!is_a_live_job_board(
        &sample([None; 5]),
        &hosts,
        now,
        Some(old)
    ));
    assert!(!is_a_live_job_board(&sample([None; 5]), &hosts, now, None));
    // Too few postings.
    assert!(!is_a_live_job_board(
        &sample([Some(recent); 5])[..4],
        &hosts,
        now,
        Some(recent)
    ));
}

#[test]
fn feed_configs_map_dates_by_default() {
    let endpoint: Url = "https://board.example/feed".parse().unwrap();
    let item = |pairs: &[(&str, &str)]| {
        rss::FeedItem(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        )
    };
    let rss_feed = [item(&[
        ("title", "A"),
        ("link", "https://board.example/a"),
        ("pubDate", "Mon, 26 Oct 2026 08:00:00 +0000"),
    ])];
    assert_eq!(
        propose_feed(&endpoint, &rss_feed)
            .fields
            .posted_at
            .as_deref(),
        Some("pubDate")
    );
    let dc = [item(&[
        ("title", "A"),
        ("link", "x"),
        ("dc:date", "2026-10-26T08:00:00Z"),
    ])];
    assert_eq!(
        propose_feed(&endpoint, &dc).fields.posted_at.as_deref(),
        Some("dc:date")
    );
    let atom = [item(&[
        ("title", "A"),
        ("link", "x"),
        ("published", "2026-10-26T08:00:00Z"),
        ("updated", "2026-10-27T08:00:00Z"),
    ])];
    assert_eq!(
        propose_feed(&endpoint, &atom).fields.posted_at.as_deref(),
        Some("published")
    );
    let updated = [item(&[
        ("title", "A"),
        ("link", "x"),
        ("updated", "2026-10-27T08:00:00Z"),
    ])];
    assert_eq!(
        propose_feed(&endpoint, &updated)
            .fields
            .posted_at
            .as_deref(),
        Some("updated")
    );
    // No date in the sample: mapped anyway.
    let none = [item(&[("title", "A"), ("link", "x")])];
    assert_eq!(
        propose_feed(&endpoint, &none).fields.posted_at.as_deref(),
        Some("pubDate")
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn candidates_checked_under_older_rules_are_checked_again_oldest_first(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let settings = crate::SourcesConfig {
        // Half of it, two, is what the waiting candidates get (#113).
        discovery_max_checks: Some(4),
        discovery_list_url: format!("{}/missing.md", site.uri()).parse().unwrap(),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    // Three checked under old rules (the oldest check first: a, b, c) and one
    // already under the current rules.
    let stamps = [
        ("board-a", "2026-09-01T00:00:00Z", 0),
        ("board-b", "2026-09-10T00:00:00Z", 0),
        ("board-c", "2026-09-20T00:00:00Z", 0),
        ("board-d", "2026-08-01T00:00:00Z", RULES),
    ];
    for (n, (name, at, rules)) in stamps.into_iter().enumerate() {
        let id = candidate(&h.db, &site, n + 1, "discovery").await;
        sqlx::query("UPDATE sources.job_boards SET slug = $2, report = $3 WHERE id = $1")
            .bind(id)
            .bind(name)
            .bind(sqlx::types::Json(
                serde_json::json!({ "checked_at": at, "rules": rules }),
            ))
            .execute(&h.db)
            .await
            .unwrap();
    }
    discover(&h.deps, &h.db).await.unwrap();
    let rules_of = |slug: &'static str| {
        let db = h.db.clone();
        async move {
            let row: (serde_json::Value,) =
                sqlx::query_as("SELECT report FROM sources.job_boards WHERE slug = $1")
                    .bind(slug)
                    .fetch_one(&db)
                    .await
                    .unwrap();
            (
                row.0["rules"].as_u64().unwrap_or(0),
                row.0["checked_at"].as_str().unwrap_or("").to_owned(),
            )
        }
    };
    assert_eq!(rules_of("board-a").await.0, u64::from(RULES));
    assert_eq!(rules_of("board-b").await.0, u64::from(RULES));
    assert_eq!(
        rules_of("board-c").await,
        (0, "2026-09-20T00:00:00Z".to_owned()),
        "past the day's two checks"
    );
    assert_eq!(
        rules_of("board-d").await.1,
        "2026-08-01T00:00:00Z",
        "already under the current rules"
    );
    // The third, on the next run.
    discover(&h.deps, &h.db).await.unwrap();
    assert_eq!(rules_of("board-c").await.0, u64::from(RULES));
    // Each is checked once with the new rules, then left alone.
    let before = site.received_requests().await.unwrap().len();
    discover(&h.deps, &h.db).await.unwrap();
    let after = site.received_requests().await.unwrap().len();
    assert_eq!(before, after - 1, "only the list was asked for");
}

/// A list offering `n` new sites, each at its own loopback address (nothing
/// listens there, so a check ends at once), served by `site`.
async fn list_of_new_sites(site: &MockServer, n: usize) {
    let items: String = (2..2 + n)
        .map(|i| format!("- [New site {i}](http://127.0.0.{i}:1/) - Jobs.\n"))
        .collect();
    mount(
        site,
        "/list.md",
        &format!("## Remote\n\n{items}"),
        "text/markdown",
    )
    .await;
}

async fn run_of_discovery(h: &Harness) -> (i32, i32, i32) {
    sqlx::query_as("SELECT found, checked, rechecked_candidates FROM sources.discovery_runs")
        .fetch_one(&h.db)
        .await
        .unwrap()
}

/// `n` candidates checked under old rules, the check dates a day apart from
/// 1 September (slugs `old-1`, `old-2`...).
async fn old_candidates(h: &Harness, site: &MockServer, n: usize) {
    for i in 1..=n {
        let id = candidate(&h.db, site, i, "discovery").await;
        sqlx::query("UPDATE sources.job_boards SET slug = $2, report = $3 WHERE id = $1")
            .bind(id)
            .bind(format!("old-{i}"))
            .bind(sqlx::types::Json(
                serde_json::json!({ "checked_at": format!("2026-09-{i:02}T00:00:00Z"), "rules": 0 }),
            ))
            .execute(&h.db)
            .await
            .unwrap();
    }
}

async fn report_of(h: &Harness, slug: &str) -> serde_json::Value {
    let row: (Option<serde_json::Value>,) =
        sqlx::query_as("SELECT report FROM sources.job_boards WHERE slug = $1")
            .bind(slug)
            .fetch_one(&h.db)
            .await
            .unwrap();
    row.0.unwrap_or_default()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn old_checks_take_half_the_day_and_new_finds_the_rest(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    list_of_new_sites(&site, 10).await;
    let settings = crate::SourcesConfig {
        discovery_max_checks: Some(6),
        discovery_list_url: format!("{}/list.md", site.uri()).parse().unwrap(),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    old_candidates(&h, &site, 5).await;
    discover(&h.deps, &h.db).await.unwrap();

    for slug in ["old-1", "old-2", "old-3"] {
        assert_eq!(report_of(&h, slug).await["rules"], RULES, "{slug}");
    }
    for slug in ["old-4", "old-5"] {
        assert_eq!(report_of(&h, slug).await["rules"], 0, "{slug}: next day");
    }
    // Three old ones (half of six) and the other three for new finds.
    assert_eq!(run_of_discovery(&h).await, (3, 6, 3));
    let new: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sources.job_boards WHERE host LIKE '127.%'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(new, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn with_no_old_work_new_finds_get_every_check(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    list_of_new_sites(&site, 10).await;
    let settings = crate::SourcesConfig {
        discovery_max_checks: Some(4),
        discovery_list_url: format!("{}/list.md", site.uri()).parse().unwrap(),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    discover(&h.deps, &h.db).await.unwrap();
    assert_eq!(run_of_discovery(&h).await, (4, 4, 0));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn never_checked_candidates_come_before_older_rules(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    let settings = crate::SourcesConfig {
        discovery_max_checks: Some(4),
        discovery_list_url: format!("{}/missing.md", site.uri()).parse().unwrap(),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    old_candidates(&h, &site, 2).await;
    // One never checked, with a config of its own.
    let fresh = candidate(&h.db, &site, 3, "suggested").await;
    sqlx::query("UPDATE sources.job_boards SET slug = 'fresh', config = $2 WHERE id = $1")
        .bind(fresh)
        .bind(sqlx::types::Json(
            serde_json::json!({ "endpoint": format!("{}/feed.xml", site.uri()), "fields": {} }),
        ))
        .execute(&h.db)
        .await
        .unwrap();
    discover(&h.deps, &h.db).await.unwrap();

    // Half of four is two: the never-checked one, then the oldest old one.
    assert!(report_of(&h, "fresh").await["checked_at"].is_string());
    assert_eq!(report_of(&h, "old-1").await["rules"], RULES);
    assert_eq!(report_of(&h, "old-2").await["rules"], 0);
    // `rechecked_candidates` counts only the older-rules ones.
    assert_eq!(run_of_discovery(&h).await, (0, 2, 1));
}

// Redirects within the site (#115).

async fn moved(server: &MockServer, from: &str, to: &str, status: u16) {
    Mock::given(method("GET"))
        .and(path(from))
        .respond_with(ResponseTemplate::new(status).insert_header("Location", to))
        .mount(server)
        .await;
}

fn dated_feed(server: &MockServer) -> String {
    feed(server, 6, Some("Sat, 24 Oct 2026 08:00:00 +0000"))
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_homepage_that_moves_is_read_where_it_went_and_the_check_goes_on_from_there(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    moved(&site, "/", "/en/", 301).await;
    // Its feed link is relative: it only reads against the final address.
    mount(
        &site,
        "/en/",
        "<html lang=\"en\"><head><link rel=\"alternate\" type=\"application/rss+xml\" \
         href=\"jobs-feed.xml\"></head><body>Jobs for people.</body></html>",
        "text/html",
    )
    .await;
    mount(
        &site,
        "/en/jobs-feed.xml",
        &dated_feed(&site),
        "application/rss+xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert!(
        report
            .found_url
            .as_deref()
            .is_some_and(|u| u.ends_with("/en/jobs-feed.xml")),
        "{:?} / {:?}",
        report.found_url,
        report.waiting_because
    );
    assert_eq!(report.language.as_deref(), Some("en"));
    assert!(report.moved_to.is_none());
    assert_eq!(asked(&site, "/").await, 1);
    assert_eq!(asked(&site, "/en/").await, 1);
    assert_eq!(row.status(), Status::Approved);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_feed_that_moves_is_read_and_the_proposed_config_names_where_it_is(db: PgPool) {
    let h = harness(db).await;
    for (n, origin) in [(1, "seed"), (2, "discovery")] {
        let site = MockServer::start().await;
        robots(&site, "User-agent: *\nDisallow: /private\n").await;
        mount(&site, "/", HOME, "text/html").await;
        moved(&site, "/feed", "/feed/", 301).await;
        mount(&site, "/feed/", &dated_feed(&site), "application/rss+xml").await;
        let id = candidate(&h.db, &site, n, origin).await;
        let row = checked(&h, id).await;
        let report = row.report().unwrap();
        let final_url = format!("{}/feed/", site.uri());
        assert_eq!(report.found_url.as_deref(), Some(final_url.as_str()));
        assert_eq!(report.sample.len(), 6, "{origin}");
        assert_eq!(asked(&site, "/feed").await, 1);
        assert_eq!(asked(&site, "/feed/").await, 1);
        // Offered to the owner for a seeded board, applied for a discovered one:
        // either way the address is the final one.
        let endpoint = if origin == "seed" {
            report.proposal.unwrap().config["endpoint"]
                .as_str()
                .map(str::to_owned)
        } else {
            row.config().unwrap().endpoint.map(|u| u.to_string())
        };
        assert_eq!(endpoint.as_deref(), Some(final_url.as_str()), "{origin}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn three_hops_are_followed_and_a_fourth_is_not(db: PgPool) {
    let h = harness(db).await;
    // /jobs.rss -> /h1 -> /h2 -> /h3 reads; one more hop does not.
    for (n, last_hop) in [(1, "/h3"), (2, "/h4")] {
        let site = MockServer::start().await;
        robots(&site, "User-agent: *\nDisallow: /private\n").await;
        mount(&site, "/", HOME, "text/html").await;
        moved(&site, "/jobs.rss", "/h1", 302).await;
        moved(&site, "/h1", "/h2", 307).await;
        moved(&site, "/h2", "/h3", 308).await;
        if last_hop == "/h4" {
            moved(&site, "/h3", "/h4", 301).await;
        }
        mount(&site, last_hop, &dated_feed(&site), "application/rss+xml").await;
        let id = candidate(&h.db, &site, n, "discovery").await;
        let report = checked(&h, id).await.report().unwrap();
        if last_hop == "/h3" {
            assert!(
                report
                    .found_url
                    .as_deref()
                    .is_some_and(|u| u.ends_with("/h3")),
                "{:?}",
                report.found_url
            );
        } else {
            assert!(report.found_url.is_none(), "{:?}", report.found_url);
            assert_eq!(asked(&site, "/h3").await, 1);
            assert_eq!(asked(&site, "/h4").await, 0, "the fourth hop is not asked");
        }
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_redirect_to_another_site_is_not_followed_and_the_homepages_is_recorded(db: PgPool) {
    let elsewhere = MockServer::start().await;
    mount(&elsewhere, "/", HOME, "text/html").await;
    mount(
        &elsewhere,
        "/jobs.rss",
        &dated_feed(&elsewhere),
        "application/rss+xml",
    )
    .await;
    // The same machine under another name: another site.
    let other = elsewhere.uri().replace("127.0.0.1", "localhost");
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    moved(&site, "/", &format!("{other}/"), 301).await;
    moved(&site, "/jobs.rss", &format!("{other}/jobs.rss"), 301).await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let row = checked(&h, id).await;
    let report = row.report().unwrap();
    assert_eq!(report.moved_to.as_deref(), Some("localhost"));
    assert!(report.found_url.is_none());
    assert_eq!(
        elsewhere.received_requests().await.unwrap().len(),
        0,
        "nothing is asked of the other site"
    );
    assert_ne!(row.status(), Status::Approved);
    assert_eq!(
        report.waiting_because.unwrap().code,
        WaitingCode::NoJobList,
        "the waiting reason is the same as for any site with no list"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_hop_the_sites_rules_disallow_is_not_read(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /moved/\n").await;
    mount(&site, "/", HOME, "text/html").await;
    moved(&site, "/jobs.rss", "/moved/jobs.rss", 301).await;
    mount(
        &site,
        "/moved/jobs.rss",
        &dated_feed(&site),
        "application/rss+xml",
    )
    .await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    assert_eq!(asked(&site, "/jobs.rss").await, 1);
    assert_eq!(asked(&site, "/moved/jobs.rss").await, 0);
    assert!(report.found_url.is_none());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn every_hop_counts_against_the_request_budget(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    // The homepage links to no terms: the usual paths are tried, and the
    // first one goes round three hops.
    mount(&site, "/", HOME, "text/html").await;
    moved(&site, "/terms", "/t1", 301).await;
    moved(&site, "/t1", "/t2", 301).await;
    moved(&site, "/t2", "/t3", 301).await;
    let settings = crate::SourcesConfig {
        discovery_max_requests: Some(5),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    // robots.txt and the homepage, then 5: /terms and its three hops, and
    // the next usual path. No more.
    assert_eq!(site.received_requests().await.unwrap().len(), 2 + 5);
    assert_eq!(asked(&site, "/t3").await, 1);
    assert_eq!(asked(&site, "/terms-of-service").await, 1);
    assert_eq!(asked(&site, "/tos").await, 0);
    assert_eq!(
        report.waiting_because.unwrap().code,
        WaitingCode::RequestLimit
    );
}

async fn answering(server: &MockServer, route: &str, status: u16) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status))
        .mount(server)
        .await;
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_429_on_robots_stops_every_later_request_to_the_host(db: PgPool) {
    let site = MockServer::start().await;
    answering(&site, "/robots.txt", 429).await;
    mount(&site, "/", HOME, "text/html").await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    assert_eq!(site.received_requests().await.unwrap().len(), 1);
    assert_eq!(asked(&site, "/").await, 0);
    assert_eq!(
        report.waiting_because.unwrap().code,
        WaitingCode::AskedToSlowDown
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_503_on_the_homepage_stops_the_rest(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    answering(&site, "/", 503).await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    // robots.txt and the homepage only.
    assert_eq!(site.received_requests().await.unwrap().len(), 2);
    assert_eq!(
        report.waiting_because.unwrap().code,
        WaitingCode::AskedToSlowDown
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn slowed_down_pages_do_not_spend_the_request_budget(db: PgPool) {
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    mount(&site, "/", HOME, "text/html").await;
    Mock::given(method("GET"))
        .and(path_regex(".*"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&site)
        .await;
    let settings = crate::SourcesConfig {
        discovery_max_requests: Some(7),
        ..boards_config()
    };
    let h = Harness::new(db, settings, None, Fake::ready()).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    // robots.txt, the homepage, and the one page that said slow down.
    assert_eq!(site.received_requests().await.unwrap().len(), 3);
    assert_eq!(
        report.waiting_because.unwrap().code,
        WaitingCode::AskedToSlowDown,
        "not the request limit"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn another_host_is_still_read_and_a_list_found_keeps_its_usual_reason(db: PgPool) {
    let elsewhere = MockServer::start().await;
    mount(
        &elsewhere,
        "/jobs.rss",
        &dated_feed(&elsewhere),
        "application/rss+xml",
    )
    .await;
    // The same machine under another name: another host.
    let other = elsewhere.uri().replace("127.0.0.1", "localhost");
    let site = MockServer::start().await;
    robots(&site, "User-agent: *\nDisallow: /private\n").await;
    let home = format!(
        "<html lang=\"en\"><head><link rel=\"alternate\" type=\"application/rss+xml\" \
         href=\"{other}/jobs.rss\"></head><body><a href=\"/terms\">Terms</a></body></html>"
    );
    mount(&site, "/", &home, "text/html").await;
    answering(&site, "/terms", 429).await;
    let h = harness(db).await;
    let id = candidate(&h.db, &site, 1, "discovery").await;
    let report = checked(&h, id).await.report().unwrap();
    assert_eq!(asked(&site, "/terms").await, 1);
    assert_eq!(asked(&elsewhere, "/jobs.rss").await, 1);
    assert!(report.found_url.is_some());
    assert_ne!(
        report.waiting_because.map(|w| w.code),
        Some(WaitingCode::AskedToSlowDown)
    );
}
