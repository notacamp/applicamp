//! The hiring systems companies publish their jobs on, and the addresses of
//! their company pages (#87).
//!
//! Seven systems, each with a public reader ([`crate::readers`]): Greenhouse,
//! Lever, Ashby, Workable, Recruitee, Teamtailor and Personio. An apply link
//! on one of them names a company page ([`company_link`]); the registry keeps
//! one row per system (its robots rules, terms and content signal, decided
//! once for every company on it) and one per company page
//! ([`crate::company_pages`]).
//!
//! **SmartRecruiters is left out on purpose**: its robots.txt allows only
//! LinkedInBot, so no other reader may read its job pages.

use url::Url;

use crate::{
    job::{Job, Source},
    readers::is_token,
};

/// A hiring system.
#[derive(Debug, Clone, Copy)]
pub struct System {
    /// What reads it.
    pub source: Source,
    /// Its row's slug in the registry.
    pub slug: &'static str,
    /// Its name.
    pub name: &'static str,
    /// Its own site, where its terms are linked from.
    pub homepage: &'static str,
    /// Its row's host in the registry (one row per host).
    pub host: &'static str,
    /// The hosts whose robots.txt the system's check reads: the first is
    /// where the reader asks, the others are where its job pages are (their
    /// content signal counts too).
    pub robots_hosts: &'static [&'static str],
    /// A company whose page the check reads its rules for (a system whose
    /// companies are on subdomains has no host of its own to read).
    pub probe: &'static str,
    /// The host to search a company page of (`site:<host>`), for the web
    /// search.
    pub search_host: &'static str,
}

/// Every hiring system read.
pub const SYSTEMS: [System; 7] = [
    System {
        source: Source::Greenhouse,
        slug: "hiring-greenhouse",
        name: "Greenhouse",
        homepage: "https://www.greenhouse.com/",
        host: "boards.greenhouse.io",
        robots_hosts: &["boards-api.greenhouse.io", "boards.greenhouse.io"],
        probe: "greenhouse",
        search_host: "boards.greenhouse.io",
    },
    System {
        source: Source::Lever,
        slug: "hiring-lever",
        name: "Lever",
        homepage: "https://www.lever.co/",
        host: "jobs.lever.co",
        robots_hosts: &["api.lever.co", "jobs.lever.co"],
        probe: "leverdemo",
        search_host: "jobs.lever.co",
    },
    System {
        source: Source::Ashby,
        slug: "hiring-ashby",
        name: "Ashby",
        homepage: "https://www.ashbyhq.com/",
        host: "jobs.ashbyhq.com",
        robots_hosts: &["api.ashbyhq.com", "jobs.ashbyhq.com"],
        probe: "ashby",
        search_host: "jobs.ashbyhq.com",
    },
    System {
        source: Source::Workable,
        slug: "hiring-workable",
        name: "Workable",
        homepage: "https://www.workable.com/",
        host: "apply.workable.com",
        robots_hosts: &["apply.workable.com"],
        probe: "huggingface",
        search_host: "apply.workable.com",
    },
    System {
        source: Source::Recruitee,
        slug: "hiring-recruitee",
        name: "Recruitee",
        homepage: "https://recruitee.com/",
        host: "recruitee.com",
        robots_hosts: &["personio.recruitee.com"],
        probe: "personio",
        search_host: "recruitee.com",
    },
    System {
        source: Source::Teamtailor,
        slug: "hiring-teamtailor",
        name: "Teamtailor",
        homepage: "https://www.teamtailor.com/",
        host: "teamtailor.com",
        robots_hosts: &["career.teamtailor.com"],
        probe: "career",
        search_host: "teamtailor.com",
    },
    System {
        source: Source::Personio,
        slug: "hiring-personio",
        name: "Personio",
        homepage: "https://www.personio.de/",
        host: "jobs.personio.de",
        robots_hosts: &["demo.jobs.personio.de"],
        probe: "demo",
        search_host: "jobs.personio.de",
    },
];

/// The system `source` reads.
pub fn of(source: Source) -> Option<&'static System> {
    SYSTEMS.iter().find(|system| system.source == source)
}

/// The system named `name` (`lever`).
pub fn named(name: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|system| system.source.as_str() == name)
}

/// The path a reader asks `source` for the jobs of company `token`
/// (what robots.txt must allow).
pub fn read_path(source: Source, token: &str) -> String {
    match source {
        Source::Greenhouse => format!("/v1/boards/{token}/jobs"),
        Source::Lever => format!("/v0/postings/{token}"),
        Source::Ashby => format!("/posting-api/job-board/{token}"),
        Source::Workable => format!("/api/v1/widget/accounts/{token}"),
        Source::Recruitee => "/api/offers".to_owned(),
        Source::Teamtailor => "/jobs.rss".to_owned(),
        Source::Personio => "/xml".to_owned(),
        Source::Arbeitnow | Source::Hn | Source::Board => "/".to_owned(),
    }
}

/// The page where a person sees a company's jobs on its system.
pub fn page_url(source: Source, token: &str, eu: bool) -> String {
    match source {
        Source::Greenhouse => format!("https://boards.greenhouse.io/{token}"),
        Source::Lever if eu => format!("https://jobs.eu.lever.co/{token}"),
        Source::Lever => format!("https://jobs.lever.co/{token}"),
        Source::Ashby => format!("https://jobs.ashbyhq.com/{token}"),
        Source::Workable => format!("https://apply.workable.com/{token}"),
        Source::Recruitee => format!("https://{token}.recruitee.com"),
        Source::Teamtailor => format!("https://{token}.teamtailor.com/jobs"),
        Source::Personio => format!("https://{token}.jobs.personio.de"),
        Source::Arbeitnow | Source::Hn | Source::Board => String::new(),
    }
}

/// A company page a link names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompanyLink {
    /// Its hiring system.
    pub source: Source,
    /// The company on it.
    pub token: String,
    /// On Lever's EU instance.
    pub eu: bool,
}

/// First path segments that are never a company: the embeds, the APIs and
/// the system's own pages.
const NOT_COMPANIES: [&str; 14] = [
    "embed",
    "api",
    "v0",
    "v1",
    "j",
    "jobs",
    "static",
    "assets",
    "js",
    "css",
    "posting-api",
    "job-board",
    "job_board",
    "robots.txt",
];

/// Subdomains that are never a company.
const NOT_SUBDOMAINS: [&str; 12] = [
    "www",
    "app",
    "api",
    "support",
    "help",
    "docs",
    "status",
    "blog",
    "careers",
    "go",
    "info",
    "marketplace",
];

/// The company page `url` is on, when it is on a hiring system's public
/// address: `boards.greenhouse.io/<t>`, `job-boards.greenhouse.io/<t>`,
/// `jobs.lever.co/<t>`, `jobs.eu.lever.co/<t>`, `jobs.ashbyhq.com/<t>`,
/// `apply.workable.com/<t>`, `<t>.recruitee.com`, `<t>.teamtailor.com`,
/// `<t>.jobs.personio.de` and `<t>.jobs.personio.com`. No request is made:
/// the address alone says it.
pub fn company_link(url: &Url) -> Option<CompanyLink> {
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let first = || {
        url.path_segments()
            .and_then(|mut segments| segments.find(|s| !s.is_empty()))
            .filter(|s| is_token(s) && !NOT_COMPANIES.contains(&s.to_ascii_lowercase().as_str()))
            .map(str::to_owned)
    };
    let on_path = |source: Source, eu: bool| first().map(|token| CompanyLink { source, token, eu });
    let subdomain = |source: Source, suffixes: &[&str]| {
        let label = suffixes
            .iter()
            .find_map(|suffix| host.strip_suffix(*suffix))?;
        let valid = !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !NOT_SUBDOMAINS.contains(&label);
        valid.then(|| CompanyLink {
            source,
            token: label.to_owned(),
            eu: false,
        })
    };
    match host.as_str() {
        "boards.greenhouse.io"
        | "job-boards.greenhouse.io"
        | "boards.eu.greenhouse.io"
        | "job-boards.eu.greenhouse.io" => on_path(Source::Greenhouse, false),
        "jobs.lever.co" => on_path(Source::Lever, false),
        "jobs.eu.lever.co" => on_path(Source::Lever, true),
        "jobs.ashbyhq.com" => on_path(Source::Ashby, false),
        "apply.workable.com" => on_path(Source::Workable, false),
        _ => subdomain(Source::Recruitee, &[".recruitee.com"])
            .or_else(|| subdomain(Source::Teamtailor, &[".teamtailor.com"]))
            .or_else(|| {
                subdomain(
                    Source::Personio,
                    &[".jobs.personio.de", ".jobs.personio.com"],
                )
            }),
    }
}

/// The company pages the links of `jobs` name, each once, in the order the
/// jobs give them: a job's own link, its apply link and the links it was
/// seen on.
pub fn company_links(jobs: &[Job]) -> Vec<CompanyLink> {
    let mut found: Vec<CompanyLink> = Vec::new();
    let mut add = |raw: &str| {
        if let Some(link) = Url::parse(raw).ok().as_ref().and_then(company_link) {
            let known = found.iter().any(|f| {
                f.source == link.source
                    && f.eu == link.eu
                    && f.token.eq_ignore_ascii_case(&link.token)
            });
            if !known {
                found.push(link);
            }
        }
    };
    for job in jobs {
        add(&job.url);
        if let Some(board) = &job.board {
            if let Some(apply) = &board.apply_url {
                add(apply);
            }
            for link in &board.links {
                add(&link.url);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(raw: &str) -> Option<(Source, String, bool)> {
        company_link(&Url::parse(raw).unwrap()).map(|l| (l.source, l.token, l.eu))
    }

    #[test]
    fn every_system_is_recognised_by_its_public_addresses() {
        let cases = [
            (
                "https://boards.greenhouse.io/acme/jobs/123",
                Source::Greenhouse,
                "acme",
                false,
            ),
            (
                "https://job-boards.greenhouse.io/acme",
                Source::Greenhouse,
                "acme",
                false,
            ),
            (
                "https://jobs.lever.co/acme/0b6d-4c1f",
                Source::Lever,
                "acme",
                false,
            ),
            ("https://jobs.eu.lever.co/acme", Source::Lever, "acme", true),
            (
                "https://jobs.ashbyhq.com/acme/9f1c",
                Source::Ashby,
                "acme",
                false,
            ),
            (
                "https://apply.workable.com/acme/j/ABC123/",
                Source::Workable,
                "acme",
                false,
            ),
            (
                "https://acme.recruitee.com/o/engineer",
                Source::Recruitee,
                "acme",
                false,
            ),
            (
                "https://acme.teamtailor.com/jobs/1-engineer",
                Source::Teamtailor,
                "acme",
                false,
            ),
            (
                "https://acme.jobs.personio.de/job/42",
                Source::Personio,
                "acme",
                false,
            ),
            (
                "https://acme.jobs.personio.com/job/42",
                Source::Personio,
                "acme",
                false,
            ),
        ];
        for (raw, source, token, eu) in cases {
            assert_eq!(link(raw), Some((source, token.to_owned(), eu)), "{raw}");
        }
    }

    #[test]
    fn what_is_not_a_company_page_is_not_one() {
        for raw in [
            "https://example.com/acme",
            "https://boards.greenhouse.io/embed/job_app?for=acme",
            "https://boards.greenhouse.io/",
            "https://apply.workable.com/j/ABC123",
            "https://www.recruitee.com/pricing",
            "https://careers.recruitee.com/",
            "https://jobs.personio.de/",
            "https://jobs.smartrecruiters.com/Acme/123",
            "ftp://jobs.lever.co/acme",
            "https://evil.example/jobs.lever.co/acme",
            "https://jobs.lever.co.evil.example/acme",
        ] {
            assert_eq!(link(raw), None, "{raw}");
        }
    }

    #[test]
    fn the_links_of_jobs_name_each_company_once() {
        let job = |url: &str, apply: Option<&str>| Job {
            source: Source::Board,
            source_ref: url.to_owned(),
            title: "Engineer".into(),
            company: "Acme".into(),
            location: None,
            work_mode: None,
            board_remote: None,
            url: url.to_owned(),
            posted_at: None,
            salary_text: None,
            description: String::new(),
            board: apply.map(|a| crate::job::BoardJob {
                apply_url: Some(a.to_owned()),
                ..crate::job::BoardJob::default()
            }),
        };
        let jobs = [
            job(
                "https://board.example/1",
                Some("https://jobs.lever.co/Acme/1"),
            ),
            job(
                "https://board.example/2",
                Some("https://jobs.lever.co/acme/2"),
            ),
            job("https://acme.recruitee.com/o/3", None),
            job("https://board.example/4", Some("https://example.com/apply")),
        ];
        let links = company_links(&jobs);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].token, "Acme", "the first spelling stays");
        assert_eq!(links[1].source, Source::Recruitee);
    }

    #[test]
    fn smartrecruiters_is_not_a_hiring_system_here() {
        assert!(SYSTEMS.iter().all(|s| s.slug != "hiring-smartrecruiters"));
        assert_eq!(SYSTEMS.len(), 7);
        for system in &SYSTEMS {
            assert_eq!(of(system.source).map(|s| s.slug), Some(system.slug));
            assert!(crate::job_boards::is_slug(system.slug));
        }
    }
}
