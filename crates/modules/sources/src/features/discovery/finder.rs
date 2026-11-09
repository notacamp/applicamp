//! Where a check looks for a site's list of jobs (#108), in the order that
//! costs least and says most. Every address is asked for once, only when the
//! site's rules allow it, and all of them share the check's request budget
//! (`APPLICAMP_SOURCES__DISCOVERY_MAX_REQUESTS`, 25 after robots.txt and the
//! homepage).
//!
//! 1. the feeds the homepage links to;
//! 2. a **job-board platform** recognised from its markup (WP Job Manager,
//!    Job Boardly, JBoard, Niceboard), read through the public address that
//!    platform has for its jobs;
//! 3. the site's own **Jobs page**, linked from the homepage, with its feed
//!    links;
//! 4. the **sitemaps** robots.txt lists (else `/sitemap.xml`), followed
//!    through sitemap indexes down to the sitemap of jobs, with a few pages
//!    read for `JobPosting` JSON-LD as the sample;
//! 5. the common **feed and API paths** ([`super::PROBES`]).
//!
//! The terms page is looked for in between: the homepage's link, the Jobs
//! page's link, then the usual paths ([`TERMS_PATHS`]).

use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use crate::{
    hiring_systems::{self, CompanyLink},
    job::{Job, html_to_text},
    job_boards::host_of,
    readers::{MAX_BOARD_BODY, MAX_PAGE, jsonld, mapping::BoardRef, rss},
};

use super::{ANCHOR, Found, PROBES, Visits, read_feed, read_sitemap};

/// Paths tried for the terms page when the site links to none.
pub const TERMS_PATHS: [&str; 5] = [
    "/terms",
    "/terms-of-service",
    "/tos",
    "/legal",
    "/terms-and-conditions",
];

/// Sitemaps read at most (those robots.txt lists), and the sitemaps of jobs
/// followed from one index.
const SITEMAPS: usize = 3;
const INDEX_CHILDREN: usize = 2;

type Allowed<'a> = &'a (dyn Fn(&Url) -> bool + Sync);

static JOBISH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)job|career|vacanc|position|opening|posting|vaga|emprego|empleo|stellen|offre|emploi",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static JOBS_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(?:all |browse |find |view |see |open |latest )?(?:jobs|careers|vacancies|positions|openings|job board|vagas|empleos|stellenangebote)\s*$",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static JOBS_HREF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^/(?:jobs|careers|vacancies|positions|openings|job-board|find-jobs|browse-jobs|all-jobs|vagas|empleos)/?$",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static TITLE_TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:title|h1)\b[^>]*>(.{0,200}?)</(?:title|h1)>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static TERMS_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)terms|conditions|\btos\b|agreement|legal|nutzungsbedingungen|\bagb\b|termos|condiciones")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// A job-board platform, recognised from the markup of its sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// WordPress with the WP Job Manager plugin.
    WpJobManager,
    /// Job Boardly.
    JobBoardly,
    /// JBoard.
    JBoard,
    /// Niceboard: no public feed, its jobs are JSON-LD pages in a sitemap.
    Niceboard,
}

impl Platform {
    /// The platform whose markup `html` carries.
    pub fn detect(html: &str) -> Option<Self> {
        let lower = html.to_ascii_lowercase();
        let has = |needle: &str| lower.contains(needle);
        if has("/wp-content/plugins/wp-job-manager") {
            Some(Self::WpJobManager)
        } else if has("jobboardly.com/assets/") {
            Some(Self::JobBoardly)
        } else if has("jboard-tenant.s3") || has("jboardapi") {
            Some(Self::JBoard)
        } else if has("niceboard.co/boards/") || has("niceboard.co/static-app") {
            Some(Self::Niceboard)
        } else {
            None
        }
    }

    /// What the check says it is.
    pub fn name(self) -> &'static str {
        match self {
            Self::WpJobManager => "WP Job Manager",
            Self::JobBoardly => "Job Boardly",
            Self::JBoard => "JBoard",
            Self::Niceboard => "Niceboard",
        }
    }

    /// The public addresses of its jobs, best first. WP Job Manager's pretty
    /// feed address has no query, which many sites' robots rules would
    /// disallow (`Disallow: /*?`).
    pub fn feeds(self) -> &'static [&'static str] {
        match self {
            Self::WpJobManager => &["/feed/job_feed/", "/?feed=job_feed"],
            Self::JobBoardly => &["/jobs.xml"],
            Self::JBoard => &["/rss/jobs"],
            Self::Niceboard => &[],
        }
    }

    /// Where its job pages live, for a read through the sitemap.
    pub fn job_path(self) -> Option<&'static str> {
        match self {
            Self::JBoard => Some("/jobs/"),
            Self::Niceboard => Some("/job/"),
            Self::WpJobManager | Self::JobBoardly => None,
        }
    }
}

/// Whether `path` (or a sitemap's file name) names jobs.
pub fn names_a_job(path: &str) -> bool {
    JOBISH.is_match(path)
}

/// The first path segment most of `urls` share (`/job/`), when there is one.
pub fn common_path(urls: &[Url]) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for url in urls.iter().take(50) {
        let path = url.path().trim_start_matches('/');
        let Some((first, rest)) = path.split_once('/') else {
            continue;
        };
        if first.is_empty() || rest.is_empty() {
            continue;
        }
        let key = format!("/{first}/");
        match counts.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => counts.push((key, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(path, _)| path)
}

/// The sitemaps robots.txt lists (`Sitemap:` lines), on `home`'s site.
pub fn robots_sitemaps(robots: &str, home: &Url) -> Vec<Url> {
    let mut found: Vec<Url> = Vec::new();
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("sitemap") {
            continue;
        }
        if let Ok(url) = home.join(value.trim())
            && host_of(&url) == host_of(home)
            && !found.contains(&url)
        {
            found.push(url);
        }
    }
    found
}

/// The link on `html` (the homepage) to the site's own Jobs page, on `home`'s
/// site: `/jobs`, `/careers` and the like, else a link saying "Jobs".
pub fn jobs_page_link(html: &str, home: &Url) -> Option<Url> {
    let host = host_of(home)?;
    let mut by_text: Option<Url> = None;
    for caps in ANCHOR.captures_iter(html) {
        let href = caps.get(1).map_or("", |m| m.as_str()).trim();
        let text = html_to_text(caps.get(2).map_or("", |m| m.as_str()));
        let Ok(url) = home.join(href) else {
            continue;
        };
        if host_of(&url).as_deref() != Some(host.as_str())
            || url.path() == "/"
            || url.query().is_some()
            || !matches!(url.scheme(), "http" | "https")
        {
            continue;
        }
        if JOBS_HREF.is_match(url.path()) {
            return Some(url);
        }
        if by_text.is_none() && JOBS_TEXT.is_match(&text) && url.path().matches('/').count() <= 2 {
            by_text = Some(url);
        }
    }
    by_text
}

/// Whether `html` is a terms page: a title or heading that says so, and a
/// real page (a site that answers every path with its front page does not).
pub fn looks_like_terms(html: &str) -> bool {
    html_to_text(html).chars().count() > 300
        && TITLE_TAGS
            .captures_iter(html)
            .filter_map(|caps| caps.get(1))
            .any(|heading| TERMS_HEADING.is_match(&html_to_text(heading.as_str())))
}

/// The company page every job of `sample` applies on, when they all apply on
/// the same company's page of a hiring system: the site is that company's, not
/// a job board.
pub fn one_hiring_system(sample: &[Job]) -> Option<CompanyLink> {
    if sample.len() < 3 {
        return None;
    }
    let mut only: Option<CompanyLink> = None;
    for job in sample {
        let apply = job.board.as_ref()?.apply_url.as_deref()?;
        let link = hiring_systems::company_link(&Url::parse(apply).ok()?)?;
        match &only {
            None => only = Some(link),
            Some(first)
                if first.source == link.source
                    && first.eu == link.eu
                    && first.token.eq_ignore_ascii_case(&link.token) => {}
            Some(_) => return None,
        }
    }
    only
}

/// The feed or API at `url`, if it reads as one.
pub(super) async fn try_feed(
    visits: &mut Visits<'_>,
    url: &Url,
    allowed: Allowed<'_>,
    board: &BoardRef,
) -> Option<Found> {
    if !allowed(url) {
        return None;
    }
    // The config proposed is for where the feed answers, not for the address
    // that sent us there.
    let (url, body) = visits.get_final(url, MAX_BOARD_BODY, &[]).await?;
    let modified = visits.last_modified(&url);
    read_feed(&url, &body, board, modified)
}

/// The feeds a page links to, on `home`'s site, tried in turn.
async fn linked_feeds(
    visits: &mut Visits<'_>,
    home: &Url,
    html: &str,
    allowed: Allowed<'_>,
    board: &BoardRef,
) -> Option<Found> {
    for link in rss::feed_links(html).into_iter().take(2) {
        if let Ok(url) = home.join(&link)
            && host_of(&url) == host_of(home)
            && let Some(found) = try_feed(visits, &url, allowed, board).await
        {
            return Some(found);
        }
    }
    None
}

/// The site's own Jobs page.
pub(super) struct JobsPage {
    pub url: Url,
    pub html: String,
}

/// What the first stage found.
pub(super) struct Evidence {
    pub found: Option<Found>,
    pub platform: Option<Platform>,
    pub jobs_page: Option<JobsPage>,
}

/// The first stages: the feeds the homepage links to, the platform's public
/// address, the site's Jobs page and its feeds.
pub(super) async fn evidence(
    visits: &mut Visits<'_>,
    home: &Url,
    html: &str,
    allowed: Allowed<'_>,
    board: &BoardRef,
) -> Evidence {
    let mut found = linked_feeds(visits, home, html, allowed, board).await;
    let platform = Platform::detect(html);
    if found.is_none()
        && let Some(platform) = platform
    {
        for path in platform.feeds() {
            if let Ok(url) = home.join(path)
                && let Some(read) = try_feed(visits, &url, allowed, board).await
            {
                found = Some(read);
                break;
            }
        }
    }
    let mut jobs_page = None;
    if found.is_none()
        && let Some(url) = jobs_page_link(html, home)
        && allowed(&url)
        && let Some((url, page)) = visits.get_page(&url).await
    {
        found = linked_feeds(visits, &url, &page, allowed, board).await;
        jobs_page = Some(JobsPage { url, html: page });
    }
    Evidence {
        found,
        platform,
        jobs_page,
    }
}

/// The usual paths for a terms page, the first that is one.
pub(super) async fn terms_by_path(
    visits: &mut Visits<'_>,
    home: &Url,
    allowed: Allowed<'_>,
) -> Option<(Url, String)> {
    for path in TERMS_PATHS {
        let Ok(url) = home.join(path) else {
            continue;
        };
        if !allowed(&url) {
            continue;
        }
        if let Some((url, html)) = visits.get_page(&url).await
            && looks_like_terms(&html)
        {
            return Some((url, html));
        }
    }
    None
}

/// The sitemaps (those robots.txt lists, else `/sitemap.xml`), through their
/// indexes, down to a sitemap of jobs with `JobPosting` pages.
pub(super) async fn sitemaps(
    visits: &mut Visits<'_>,
    listed: &[Url],
    job_path: Option<&str>,
    allowed: Allowed<'_>,
    board: &BoardRef,
) -> Option<Found> {
    for sitemap in listed.iter().take(SITEMAPS) {
        if !allowed(sitemap) {
            continue;
        }
        let Some((sitemap, body)) = visits.get_final(sitemap, MAX_PAGE, &[]).await else {
            continue;
        };
        let sitemap = &sitemap;
        let text = String::from_utf8_lossy(&body).into_owned();
        if text.contains("<sitemapindex") {
            let kids: Vec<Url> = jsonld::sitemap_urls(&text, None, 200)
                .into_iter()
                .filter(|kid| host_of(kid) == host_of(sitemap) && allowed(kid))
                .filter(|kid| names_a_job(kid.path()))
                .take(INDEX_CHILDREN)
                .collect();
            for kid in kids {
                let Some((kid, kid_body)) = visits.get_final(&kid, MAX_PAGE, &[]).await else {
                    continue;
                };
                if let Some(found) =
                    read_sitemap(visits, &kid, &kid_body, job_path, true, allowed, board).await
                {
                    return Some(found);
                }
            }
        } else if let Some(found) = read_sitemap(
            visits,
            sitemap,
            &body,
            job_path,
            names_a_job(sitemap.path()),
            allowed,
            board,
        )
        .await
        {
            return Some(found);
        }
    }
    None
}

/// The common feed and API paths.
pub(super) async fn blind(
    visits: &mut Visits<'_>,
    home: &Url,
    allowed: Allowed<'_>,
    board: &BoardRef,
) -> Option<Found> {
    for path in PROBES {
        if let Ok(url) = home.join(path)
            && let Some(found) = try_feed(visits, &url, allowed, board).await
        {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        raw.parse().unwrap()
    }

    #[test]
    fn platforms_are_recognised_from_their_markup() {
        let wpjm = r#"<link rel="stylesheet" href="https://jobs.example/wp-content/plugins/wp-job-manager/assets/css/frontend.css?ver=1">"#;
        assert_eq!(Platform::detect(wpjm), Some(Platform::WpJobManager));
        let boardly = r#"<link href="https://www.jobboardly.com/assets/board-67e33a7c.css">"#;
        assert_eq!(Platform::detect(boardly), Some(Platform::JobBoardly));
        let jboard =
            r#"<img src="https://jboard-tenant.s3.us-west-1.amazonaws.com/employers/x.png">"#;
        assert_eq!(Platform::detect(jboard), Some(Platform::JBoard));
        let nice = r#"<img src="https://niceboard.co/boards/example/assets/bg.png">"#;
        assert_eq!(Platform::detect(nice), Some(Platform::Niceboard));
        assert_eq!(Platform::detect("<html><body>A blog.</body></html>"), None);
        assert!(
            Platform::WpJobManager.feeds()[0].ends_with('/'),
            "no query first"
        );
        assert!(Platform::Niceboard.feeds().is_empty());
    }

    #[test]
    fn robots_lists_sitemaps_on_the_site() {
        let home = url("https://board.example/");
        let robots = "User-agent: *\nDisallow: /api/\nSitemap: https://board.example/sitemap.xml\n\
                      sitemap: /jobs-sitemap.xml # jobs\nSitemap: https://elsewhere.example/s.xml\n";
        assert_eq!(
            robots_sitemaps(robots, &home),
            [
                url("https://board.example/sitemap.xml"),
                url("https://board.example/jobs-sitemap.xml")
            ]
        );
        assert!(robots_sitemaps("User-agent: *\n", &home).is_empty());
    }

    #[test]
    fn the_jobs_page_is_the_link_that_says_so() {
        let home = url("https://board.example/");
        let html = r#"<a href="/about">About</a><a href="/blog/jobs-tips">Tips</a>
            <a href="https://other.example/jobs">Elsewhere</a><a href="/careers/">Work with us</a>"#;
        assert_eq!(
            jobs_page_link(html, &home),
            Some(url("https://board.example/careers/"))
        );
        let by_text = r#"<a href="/browse">Browse jobs</a><a href="/about">About</a>"#;
        assert_eq!(
            jobs_page_link(by_text, &home),
            Some(url("https://board.example/browse"))
        );
        assert_eq!(
            jobs_page_link(r#"<a href="/">Jobs</a><a href="/about">About</a>"#, &home),
            None
        );
    }

    #[test]
    fn a_terms_page_has_a_terms_heading_and_a_body() {
        let body = "You may use this site as described here. ".repeat(20);
        assert!(looks_like_terms(&format!(
            "<html><title>Terms of Service</title><body><p>{body}</p></body></html>"
        )));
        assert!(looks_like_terms(&format!(
            "<h1>Terms &amp; Conditions</h1><p>{body}</p>"
        )));
        // A catch-all page that answers every path with the front page.
        assert!(!looks_like_terms(&format!(
            "<title>Find jobs</title><a href=\"/terms\">Terms</a><p>{body}</p>"
        )));
        assert!(!looks_like_terms("<title>Terms</title><p>Short.</p>"));
        assert_eq!(TERMS_PATHS.len(), 5);
    }

    #[test]
    fn sitemap_paths_and_names() {
        let urls: Vec<Url> = [
            "https://board.example/job/1-engineer",
            "https://board.example/job/2-designer",
            "https://board.example/about",
            "https://board.example/job/3-analyst",
            "https://board.example/blog/post",
        ]
        .iter()
        .map(|u| url(u))
        .collect();
        assert_eq!(common_path(&urls).as_deref(), Some("/job/"));
        assert_eq!(common_path(&[]), None);
        assert!(names_a_job("/sitemap-jobs.xml") && names_a_job("/sitemaps/careers-1.xml"));
        assert!(!names_a_job("/sitemap-companies.xml") && !names_a_job("/sitemap-blog.xml"));
    }
}
