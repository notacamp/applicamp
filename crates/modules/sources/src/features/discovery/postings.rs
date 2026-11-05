//! What makes a sample item a posting, and the company a feed's titles hold.
//!
//! - A **posting** (#108) is a title plus a company, an apply link, or a link
//!   to its own page on the site's host (a page whose path names a job, never
//!   a blog post).
//! - Many feeds give no company field: the company is in the title ("Role at
//!   Company", "Company: Role", "Role - Company", "Role | Company", "Company
//!   is hiring a Role"). A pattern is applied only when it is consistent
//!   across the sample: at least 70 % of the titles match it, with at least
//!   three different companies, none of them more than half of the matches
//!   (a site's own name at the end of every title is not a company).
//!
//! Pure functions, no request.

use std::{collections::HashMap, sync::LazyLock};

use regex::Regex;
use url::Url;

use crate::{
    job::Job,
    job_boards::{TitleSplit, host_of},
    readers::mapping::split_title,
};

/// The fewest titles a pattern is looked for in.
const MIN_TITLES: usize = 3;
/// The fewest different companies a pattern must give.
const MIN_COMPANIES: usize = 3;
/// Longest company name a title may hold, in characters, and in words.
const COMPANY_MAX: (usize, usize) = (60, 5);

/// The patterns, most specific first: separator, company first, from the end.
const PATTERNS: [(&str, bool, bool); 8] = [
    (" is hiring ", true, false),
    (" at ", false, true),
    (" | ", false, true),
    (" \u{2014} ", false, true),
    (" \u{2013} ", false, true),
    (" - ", false, true),
    (": ", true, false),
    (" @ ", false, true),
];

static JOBISH_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)job|career|position|vacanc|opening|role|hiring|apply|listing|posting|vaga|emprego|empleo|stellen|offre|emploi",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static BLOGGY_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/(blog|news|article|articles|press|podcast|category|tag|author|about)(/|$)")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
fn pattern(index: usize) -> TitleSplit {
    let (separator, company_first, last) = PATTERNS[index];
    TitleSplit {
        separator: separator.to_owned(),
        company_first,
        last,
    }
}

/// Whether `company` can be a company name: short, a few words.
fn plausible_company(company: &str) -> bool {
    company.chars().count() <= COMPANY_MAX.0
        && company.split_whitespace().count() <= COMPANY_MAX.1
        && company.chars().any(char::is_alphabetic)
}

/// The pattern most of `titles` follow, if one is consistent: at least 70 %
/// of the titles match it, with at least three different companies, none
/// more than half of the matches.
pub fn infer_title_split(titles: &[&str]) -> Option<TitleSplit> {
    if titles.len() < MIN_TITLES {
        return None;
    }
    for index in 0..PATTERNS.len() {
        let split = pattern(index);
        let companies: Vec<String> = titles
            .iter()
            .filter_map(|title| split_title(title, &split))
            .map(|(company, _)| company)
            .filter(|company| plausible_company(company))
            .collect();
        if companies.len() * 10 < titles.len() * 7 {
            continue;
        }
        let mut counts: HashMap<String, usize> = HashMap::new();
        for company in &companies {
            *counts.entry(company.to_lowercase()).or_default() += 1;
        }
        let most = counts.values().copied().max().unwrap_or(0);
        if counts.len() >= MIN_COMPANIES && most * 2 <= companies.len() {
            return Some(split);
        }
    }
    None
}

/// `jobs` with the company read from the title, for the jobs a feed gives no
/// company for, when most of the sample has none and one pattern is
/// consistent. The pattern, when it was applied.
pub fn with_companies(mut jobs: Vec<Job>) -> (Vec<Job>, Option<TitleSplit>) {
    let missing: Vec<&str> = jobs
        .iter()
        .filter(|job| job.company.trim().is_empty())
        .map(|job| job.title.as_str())
        .collect();
    if missing.len() * 2 < jobs.len() {
        return (jobs, None);
    }
    let Some(split) = infer_title_split(&missing) else {
        return (jobs, None);
    };
    for job in &mut jobs {
        if job.company.trim().is_empty()
            && let Some((company, role)) = split_title(&job.title, &split)
        {
            job.company = company;
            job.title = role;
        }
    }
    (jobs, Some(split))
}

/// `job`'s link when it is on one of `hosts`.
fn own_link(job: &Job, hosts: &[String]) -> Option<Url> {
    let url = Url::parse(&job.url).ok()?;
    let host = host_of(&url)?;
    hosts.contains(&host).then_some(url)
}

/// Whether `job`'s link is its own page on one of `hosts` (the site's, and
/// its feed's): a page whose path names a job, not a blog post or the site's
/// front page.
fn own_page(job: &Job, hosts: &[String]) -> bool {
    own_link(job, hosts).is_some_and(|url| {
        let path = url.path();
        path.trim_matches('/')
            .contains(|c: char| c.is_alphanumeric())
            && JOBISH_PATH.is_match(path)
            && !BLOGGY_PATH.is_match(path)
    })
}

/// Whether `job`'s link is a blog post on one of `hosts`: a feed of a blog
/// has authors, not companies.
fn blog_post(job: &Job, hosts: &[String]) -> bool {
    own_link(job, hosts).is_some_and(|url| BLOGGY_PATH.is_match(url.path()))
}

/// Whether `job` is a posting: a title, and a company, an apply link, or a
/// link to its own page on one of `hosts`; never a blog post.
pub fn is_posting(job: &Job, hosts: &[String]) -> bool {
    !job.title.trim().is_empty()
        && !blog_post(job, hosts)
        && (!job.company.trim().is_empty()
            || job.board.as_ref().is_some_and(|b| b.apply_url.is_some())
            || own_page(job, hosts))
}

/// How many of `sample` are postings.
pub fn count(sample: &[Job], hosts: &[String]) -> usize {
    sample.iter().filter(|job| is_posting(job, hosts)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{BoardJob, Source};

    fn job(title: &str, company: &str, url: &str, apply: Option<&str>) -> Job {
        Job {
            source: Source::Board,
            source_ref: url.to_owned(),
            title: title.to_owned(),
            company: company.to_owned(),
            location: None,
            work_mode: None,
            board_remote: None,
            url: url.to_owned(),
            posted_at: None,
            salary_text: None,
            description: String::new(),
            board: Some(BoardJob {
                slug: "example".into(),
                links: Vec::new(),
                apply_url: apply.map(str::to_owned),
                company_url: None,
                regions: Vec::new(),
                expires_at: None,
                pay: None,
            }),
        }
    }

    fn hosts() -> Vec<String> {
        vec!["board.example".to_owned()]
    }

    #[test]
    fn a_posting_is_a_title_and_a_company_an_apply_link_or_its_own_job_page() {
        let h = hosts();
        assert!(is_posting(
            &job("Engineer", "Example Co", "https://board.example/", None),
            &h
        ));
        assert!(is_posting(
            &job(
                "Engineer",
                "",
                "https://board.example/",
                Some("https://jobs.lever.co/x/1")
            ),
            &h
        ));
        assert!(is_posting(
            &job("Engineer", "", "https://board.example/job/engineer-7", None),
            &h
        ));
        assert!(is_posting(
            &job("Engineer", "", "https://www.board.example/jobs/7/", None),
            &h
        ));
        // No title, the front page, another site, a blog post: not postings.
        assert!(!is_posting(
            &job("", "Example Co", "https://board.example/job/1", None),
            &h
        ));
        assert!(!is_posting(
            &job("Engineer", "", "https://board.example/", None),
            &h
        ));
        assert!(!is_posting(
            &job("Engineer", "", "https://elsewhere.example/job/1", None),
            &h
        ));
        assert!(!is_posting(
            &job(
                "How to hire a job coach",
                "",
                "https://board.example/blog/jobs-tips",
                None
            ),
            &h
        ));
        assert!(!is_posting(
            &job(
                "Ten tips",
                "",
                "https://board.example/2026/10/ten-tips",
                None
            ),
            &h
        ));
    }

    #[test]
    fn the_company_is_read_from_the_title_when_the_pattern_is_consistent() {
        let at = [
            "Senior Rust Engineer at Example Labs",
            "Product Designer at Acme Corp",
            "Data Analyst at Globex",
            "Support Lead at Initech ($45k-$60k)",
            "Backend Engineer at Umbrella Co",
        ];
        let split = infer_title_split(&at).unwrap();
        assert_eq!(
            (split.separator.as_str(), split.company_first, split.last),
            (" at ", false, true)
        );
        assert_eq!(
            split_title(at[3], &split),
            Some(("Initech".to_owned(), "Support Lead".to_owned())),
            "the pay is not the company"
        );

        let colon = [
            "Example Labs: Rust Engineer",
            "Acme Corp: Designer",
            "Globex: Analyst",
        ];
        let split = infer_title_split(&colon).unwrap();
        assert!(split.company_first && split.separator == ": ");

        let dash = [
            "Rust Engineer - Example Labs",
            "Designer - Acme Corp",
            "Analyst - Globex",
            "Full-Stack Developer - Initech",
        ];
        let split = infer_title_split(&dash).unwrap();
        assert!(!split.company_first && split.separator == " - ");

        let pipe = [
            "Rust Engineer | Example Labs",
            "Designer | Acme",
            "Analyst | Globex",
        ];
        assert_eq!(infer_title_split(&pipe).unwrap().separator, " | ");

        let hiring = [
            "Example Labs is hiring a Rust Engineer",
            "Acme Corp is hiring an Account Executive",
            "Globex is hiring Designers",
        ];
        let split = infer_title_split(&hiring).unwrap();
        assert_eq!(
            split_title(hiring[1], &split),
            Some(("Acme Corp".to_owned(), "Account Executive".to_owned()))
        );

        let em_dash = [
            "Kernel Intern  \u{2014} Etched",
            "Staff Engineer \u{2014} Crusoe",
            "Designer \u{2014} Acme",
        ];
        assert_eq!(infer_title_split(&em_dash).unwrap().separator, " \u{2014} ");
    }

    #[test]
    fn a_pattern_that_is_not_consistent_is_not_applied() {
        // Under 70 % follow it.
        let mixed = [
            "Rust Engineer at Example Labs",
            "Designer",
            "Analyst",
            "Support Lead at Initech",
            "Remote Developer",
        ];
        assert_eq!(infer_title_split(&mixed), None);
        // The site's own name after every title is not a company.
        let site = [
            "Rust Engineer - Board Jobs",
            "Designer - Board Jobs",
            "Analyst - Board Jobs",
            "Lead - Board Jobs",
        ];
        assert_eq!(infer_title_split(&site), None);
        // A fixed label before the colon is not a company.
        let label = [
            "Hiring: Rust Engineer",
            "Hiring: Designer",
            "Hiring: Analyst",
        ];
        assert_eq!(infer_title_split(&label), None);
        // Too few titles to say.
        assert_eq!(infer_title_split(&["A at B", "C at D"]), None);
        // A long sentence is not a company name.
        let sentences = [
            "We are looking for a great engineer to join our team at the new office in town today",
            "We are looking for a great designer to join our team at the other office in town today",
            "We are looking for a great analyst to join our team at the third office in town today",
        ];
        assert_eq!(infer_title_split(&sentences), None);
    }

    #[test]
    fn jobs_without_a_company_get_it_from_a_consistent_title() {
        let jobs: Vec<Job> = [
            "Rust Engineer at Example Labs",
            "Designer at Acme Corp",
            "Analyst at Globex",
            "Lead at Initech",
        ]
        .iter()
        .enumerate()
        .map(|(n, title)| job(title, "", &format!("https://board.example/job/{n}"), None))
        .collect();
        let (jobs, split) = with_companies(jobs);
        assert!(split.is_some());
        assert_eq!(jobs[0].company, "Example Labs");
        assert_eq!(jobs[0].title, "Rust Engineer");

        // Companies already given: left alone.
        let given = vec![
            job(
                "Engineer at X",
                "Example Co",
                "https://board.example/job/1",
                None,
            ),
            job("Designer at Y", "Acme", "https://board.example/job/2", None),
            job(
                "Analyst at Z",
                "Globex",
                "https://board.example/job/3",
                None,
            ),
        ];
        let (given, split) = with_companies(given);
        assert!(split.is_none());
        assert_eq!(given[0].title, "Engineer at X");
    }
}
