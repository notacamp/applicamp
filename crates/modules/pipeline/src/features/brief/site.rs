//! Which website is the company's. In this order:
//!
//! 1. the one the person typed (it corrects every guess),
//! 2. the one the job board gave,
//! 3. the apply link's host, when it is not a known job board or
//!    applicant-tracking host (a link-added posting's own address counts as
//!    its apply link),
//! 4. none: the person is asked once, and the answer is kept on the company.

use url::Url;

use crate::companies::{Company, SiteSource, origin_of};

/// Hosts (and their subdomains) that are job boards or applicant-tracking
/// systems: a link to one says where the job is listed, never what the
/// company's own site is.
pub const NOT_A_COMPANY: &[&str] = &[
    "greenhouse.io",
    "lever.co",
    "ashbyhq.com",
    "workable.com",
    "smartrecruiters.com",
    "recruitee.com",
    "breezy.hr",
    "bamboohr.com",
    "personio.de",
    "personio.com",
    "teamtailor.com",
    "myworkdayjobs.com",
    "jobvite.com",
    "icims.com",
    "join.com",
    "himalayas.app",
    "remoteok.com",
    "remoteok.io",
    "jobicy.com",
    "arbeitnow.com",
    "remotive.com",
    "weworkremotely.com",
    "ycombinator.com",
    "workatastartup.com",
    "wellfound.com",
    "angel.co",
    "linkedin.com",
    "indeed.com",
    "glassdoor.com",
    "monster.com",
    "ziprecruiter.com",
    "dice.com",
];

/// Labels a company puts before its name on a jobs site, not part of its own
/// site's host (`careers.example.com` is `example.com`).
const JOBS_LABELS: &[&str] = &["jobs", "careers", "apply", "hire", "join", "work"];

/// Whether `host` is a job board or tracking system (or one of its
/// subdomains), or one of `boards`' hosts (the job boards this posting was
/// seen on).
pub fn is_not_a_company(host: &str, boards: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let on = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    NOT_A_COMPANY.iter().any(|domain| on(domain))
        || boards.iter().any(|board| on(&board.to_ascii_lowercase()))
}

/// The site `link`'s host stands for: its origin, without a jobs label
/// (`careers.`) in front; none when the host is a job board or a tracker.
pub fn from_link(link: &str, boards: &[String]) -> Option<Url> {
    let url = origin_of(link)?;
    let host = url.host_str()?;
    if is_not_a_company(host, boards) {
        return None;
    }
    let mut labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 && JOBS_LABELS.contains(&labels[0]) {
        labels.remove(0);
    }
    let mut site = Url::parse(&format!("{}://{}/", url.scheme(), labels.join("."))).ok()?;
    if let Some(port) = url.port() {
        site.set_port(Some(port)).ok()?;
    }
    Some(site)
}

/// What is known about a company's website.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The site; none when the person has to type it.
    pub site: Option<Url>,
    /// Where it came from.
    pub source: Option<SiteSource>,
}

/// The company's site: `company` as stored (a person's or a board's site
/// stays), else the apply link's host; for a posting the person pasted
/// (`own_link`), its own address counts as the apply link. `boards` are the
/// hosts of the job boards the posting was seen on.
pub fn choose(
    company: Option<&Company>,
    apply_url: Option<&str>,
    posting_url: &str,
    own_link: bool,
    boards: &[String],
) -> Choice {
    if let Some(company) = company
        && let (Some(site), Some(source)) = (company.site.as_deref(), company.source())
        && let Some(site) = origin_of(site)
    {
        return Choice {
            site: Some(site),
            source: Some(source),
        };
    }
    let guess = apply_url
        .and_then(|link| from_link(link, boards))
        .or_else(|| own_link.then(|| from_link(posting_url, boards)).flatten());
    Choice {
        source: guess.as_ref().map(|_| SiteSource::ApplyHost),
        site: guess,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn company(site: Option<&str>, source: Option<&str>) -> Company {
        Company {
            id: Uuid::now_v7(),
            name: "Example Co".into(),
            site: site.map(str::to_owned),
            site_source: source.map(str::to_owned),
            read_at: Some(Utc::now()),
            pages: sqlx::types::Json(Vec::new()),
            pages_blocked: 0,
            facts_dropped: 0,
        }
    }

    fn choice(
        company: Option<&Company>,
        apply: Option<&str>,
        posting: &str,
        own_link: bool,
    ) -> (Option<String>, Option<SiteSource>) {
        let c = choose(
            company,
            apply,
            posting,
            own_link,
            &["himalayas.app".to_owned()],
        );
        (c.site.map(|s| s.to_string()), c.source)
    }

    #[test]
    fn the_board_website_comes_before_the_apply_host_and_the_person_before_both() {
        let board = company(Some("https://www.example.com/"), Some("board"));
        let (site, source) = choice(
            Some(&board),
            Some("https://jobs.other.example/apply/1"),
            "https://himalayas.app/jobs/x",
            false,
        );
        assert_eq!(site.as_deref(), Some("https://www.example.com/"));
        assert_eq!(source, Some(SiteSource::Board));

        let person = company(Some("https://typed.example/"), Some("person"));
        let (site, source) = choice(Some(&person), Some("https://example.com/a"), "", false);
        assert_eq!(site.as_deref(), Some("https://typed.example/"));
        assert_eq!(source, Some(SiteSource::Person));
    }

    #[test]
    fn a_non_tracker_apply_host_is_the_site_and_a_jobs_label_is_dropped() {
        let (site, source) = choice(
            None,
            Some("https://careers.example.com/apply/42?utm_source=api"),
            "https://himalayas.app/jobs/x",
            false,
        );
        assert_eq!(site.as_deref(), Some("https://example.com/"));
        assert_eq!(source, Some(SiteSource::ApplyHost));
        // A port stays (a laptop demo's fake site).
        let (site, _) = choice(None, Some("http://example-co.test:9020/apply/1"), "", false);
        assert_eq!(site.as_deref(), Some("http://example-co.test:9020/"));
    }

    #[test]
    fn job_boards_and_trackers_are_never_the_companys_site() {
        for apply in [
            "https://boards.greenhouse.io/exampleco/jobs/1",
            "https://jobs.lever.co/exampleco/abc",
            "https://jobs.ashbyhq.com/exampleco/abc",
            "https://apply.workable.com/exampleco/j/1",
            "https://himalayas.app/companies/example/jobs/1",
            "https://remoteok.com/remote-jobs/1",
            "https://www.linkedin.com/jobs/view/1",
            "https://uk.indeed.com/viewjob?jk=1",
            "https://news.ycombinator.com/item?id=1",
            // A board this posting was seen on, whatever the list says.
            "https://www.himalayas.app/x",
        ] {
            assert_eq!(
                choice(None, Some(apply), "", false),
                (None, None),
                "{apply}"
            );
        }
        // Not a lookalike.
        assert!(!is_not_a_company("notgreenhouse.io.example.com", &[]));
        assert!(is_not_a_company("boards.GREENHOUSE.io", &[]));
    }

    #[test]
    fn a_pasted_posting_stands_for_its_own_apply_link_but_a_boards_does_not() {
        let own = choice(None, None, "https://example.com/careers/staff", true);
        assert_eq!(own.0.as_deref(), Some("https://example.com/"));
        // A job board's posting page says where the job is listed, not who hires.
        assert_eq!(
            choice(None, None, "https://example.com/jobs/1", false),
            (None, None)
        );
        // Nothing to go on: the person is asked once.
        assert_eq!(
            choice(None, None, "https://jobs.lever.co/x/1", true),
            (None, None)
        );
    }
}
