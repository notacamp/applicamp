//! Reading a company's site: which links are picked, and that robots.txt and
//! the one-site rule are respected. A fake site (wiremock); invented text.

use std::sync::Arc;

use chrono::Utc;
use url::Url;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::*;
use crate::config::SourcesConfig;

/// A fake site on a port of its own (wiremock's pooled servers are shared
/// across the tests' runtimes).
async fn exclusive() -> MockServer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    MockServer::builder().listener(listener).start().await
}

fn reader() -> Reader {
    Reader::new(Arc::new(SourcesConfig {
        allow_private_hosts: Some(true),
        requests_per_second: Some(1_000),
        ..SourcesConfig::default()
    }))
    .unwrap()
}

fn html(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "text/html; charset=utf-8")
}

async fn serve(server: &MockServer, at: &str, body: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(html(body))
        .mount(server)
        .await;
}

fn home() -> Url {
    "https://example.test/".parse().unwrap()
}

fn paths(urls: &[Url]) -> Vec<&str> {
    urls.iter().map(Url::path).collect()
}

#[test]
fn only_same_site_pages_about_the_company_are_picked_most_wanted_first() {
    let page = r#"
        <a href="/blog/hotwire">Engineering blog</a>
        <a href="/about">About us</a>
        <a href="https://www.example.test/customers#top">Customers</a>
        <a href="https://news.example/about">About them</a>
        <a href="/pricing">See Pricing</a>
        <a href="/team.pdf">Team</a>
        <a href='/careers'>Come work with us</a>
        <a href="/about">About again</a>
        <a href="mailto:hello@example.test">Contact</a>
        <a href="/">Home</a>
        <a href="/jobs">Join the team</a>
    "#;
    let picked = linked_pages(page, &home());
    assert_eq!(
        paths(&picked),
        ["/about", "/customers", "/blog/hotwire", "/jobs", "/careers"],
        "the topics' order first; other hosts, files, mail links and repeats never"
    );
    assert!(
        picked.iter().all(|url| same_site(url, &home())),
        "every page is on the home page's site"
    );
}

#[tokio::test]
async fn a_site_is_read_where_robots_allows_and_never_beyond_it() {
    let server = exclusive().await;
    let site: Url = server.uri().parse().unwrap();
    serve(&server, "/robots.txt", "User-agent: *\nDisallow: /secret\n").await;
    serve(
        &server,
        "/",
        r#"<html><body><h1>Example Co</h1><p>We sell invented ledgers.</p>
           <a href="/about">About</a>
           <a href="/secret/customers">Customers</a>
           <a href="https://elsewhere.example/about">Elsewhere</a></body></html>"#,
    )
    .await;
    serve(&server, "/about", "<p>Founded in an invented year.</p>").await;
    // Forbidden by robots.txt: must never be asked.
    Mock::given(method("GET"))
        .and(path("/secret/customers"))
        .respond_with(html("<p>Forbidden.</p>"))
        .expect(0)
        .mount(&server)
        .await;

    let read = read_site(&reader(), &site, None, Utc::now()).await.unwrap();
    let urls: Vec<&str> = read.pages.iter().map(|p| p.url.path()).collect();
    assert_eq!(urls, ["/", "/about"]);
    assert_eq!(read.blocked, 1, "the forbidden page is counted");
    assert_eq!(read.pages[0].kind, Kind::Home);
    assert_eq!(read.pages[1].kind, Kind::Linked);
    assert!(read.pages[0].text.contains("We sell invented ledgers."));
    assert!(
        !read.pages[0].text.contains("<p>"),
        "the pages are read as text"
    );
}

#[tokio::test]
async fn the_posting_comes_first_and_has_to_be_allowed_too() {
    let server = exclusive().await;
    let site: Url = server.uri().parse().unwrap();
    serve(
        &server,
        "/robots.txt",
        "User-agent: *\nDisallow: /jobs/private\n",
    )
    .await;
    serve(&server, "/", "<p>Example Co home.</p>").await;
    serve(
        &server,
        "/jobs/staff",
        "<p>Staff Engineer. You will own billing.</p>",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/jobs/private"))
        .respond_with(html("<p>Hidden posting.</p>"))
        .expect(0)
        .mount(&server)
        .await;

    let posting: Url = format!("{}/jobs/staff", server.uri()).parse().unwrap();
    let read = read_site(&reader(), &site, Some(&posting), Utc::now())
        .await
        .unwrap();
    assert_eq!(read.pages[0].kind, Kind::Posting);
    assert!(read.pages[0].text.contains("own billing"));

    let private: Url = format!("{}/jobs/private", server.uri()).parse().unwrap();
    let read = read_site(&reader(), &site, Some(&private), Utc::now())
        .await
        .unwrap();
    assert_eq!(read.blocked, 1);
    assert!(read.pages.iter().all(|p| p.kind != Kind::Posting));
}

#[tokio::test]
async fn a_site_that_forbids_its_home_page_is_not_read_at_all() {
    let server = exclusive().await;
    let site: Url = server.uri().parse().unwrap();
    serve(&server, "/robots.txt", "User-agent: *\nDisallow: /\n").await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(html("<p>Closed.</p>"))
        .expect(0)
        .mount(&server)
        .await;
    let failure = read_site(&reader(), &site, None, Utc::now())
        .await
        .unwrap_err();
    assert_eq!(failure, Failure::RobotsDisallows);
}

#[tokio::test]
async fn a_site_that_does_not_answer_or_has_no_text_says_so() {
    let server = exclusive().await;
    let site: Url = server.uri().parse().unwrap();
    // No robots.txt, no home page: 404 everywhere.
    assert_eq!(
        read_site(&reader(), &site, None, Utc::now())
            .await
            .unwrap_err(),
        Failure::Unreachable
    );
    serve(&server, "/", "<html><script>var a = 1;</script></html>").await;
    assert_eq!(
        read_site(&reader(), &site, None, Utc::now())
            .await
            .unwrap_err(),
        Failure::NothingReadable
    );
}

#[tokio::test]
async fn a_redirect_to_another_host_is_refused() {
    let server = exclusive().await;
    let site: Url = server.uri().parse().unwrap();
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "http://other.example/"))
        .mount(&server)
        .await;
    assert_eq!(
        read_site(&reader(), &site, None, Utc::now())
            .await
            .unwrap_err(),
        Failure::Unreachable
    );
}

#[tokio::test]
async fn a_page_over_the_cap_is_cut_not_refused() {
    let server = exclusive().await;
    let big = format!("<p>{}</p>", "word ".repeat(100_000));
    serve(&server, "/big", &big).await;
    let page = reader()
        .page(&format!("{}/big", server.uri()).parse().unwrap(), 1_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.body.len(), 1_000);
}
