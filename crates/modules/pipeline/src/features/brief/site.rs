//! Which website is the company's. In this order:
//!
//! 1. the one the person typed (it corrects every guess),
//! 2. the one the job board gave,
//! 3. the apply link's host, when it is not a known job board or
//!    applicant-tracking host (a link-added posting's own address counts as
//!    its apply link),
//! 4. a link or an email address in the posting whose domain is the
//!    company's name ([`in_posting`], [`host_names`]),
//! 5. found by the brief, by asking at most [`MOST_TRIED`] home pages: the
//!    posting's other domains, then domains made from the name ([`guesses`]),
//!    each kept only when its home page names the company ([`names`]),
//! 6. none: the person is asked once, and the answer is kept on the company.
//!
//! No model is asked. The posting's text is untrusted data: what it gives is
//! only an address, read under the same rules as any other.

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

/// Hosts (and their subdomains) that are webmail, link shorteners, or sites
/// that host a company's page rather than being it: never its own site.
pub const NOT_ITS_OWN: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "outlook.com",
    "hotmail.com",
    "live.com",
    "yahoo.com",
    "icloud.com",
    "me.com",
    "proton.me",
    "protonmail.com",
    "gmx.com",
    "gmx.de",
    "gmx.net",
    "web.de",
    "aol.com",
    "zoho.com",
    "mail.com",
    "yandex.com",
    "bit.ly",
    "t.co",
    "lnkd.in",
    "goo.gl",
    "tinyurl.com",
    "ow.ly",
    "buff.ly",
    "forms.gle",
    "github.com",
    "gitlab.com",
    "twitter.com",
    "x.com",
    "facebook.com",
    "instagram.com",
    "youtube.com",
    "youtu.be",
    "tiktok.com",
    "medium.com",
    "substack.com",
    "google.com",
    "apple.com",
    "microsoft.com",
    "notion.site",
    "notion.so",
    "calendly.com",
    "typeform.com",
    "wikipedia.org",
];

/// Words of a company's name that are not in its domain: `Example Software
/// Inc` is `example.com`.
const NAME_WORDS_LEFT_OUT: &[&str] = &[
    "inc",
    "incorporated",
    "ltd",
    "limited",
    "llc",
    "llp",
    "gmbh",
    "ag",
    "sa",
    "sas",
    "sarl",
    "bv",
    "nv",
    "plc",
    "co",
    "corp",
    "corporation",
    "company",
    "software",
    "technologies",
    "technology",
    "tech",
    "labs",
    "lab",
    "group",
    "holdings",
    "studio",
    "studios",
    "systems",
    "solutions",
    "the",
    "oy",
    "ab",
    "aps",
    "srl",
    "spa",
    "lda",
    "sl",
    "pty",
];

/// Most home pages asked when looking for a company's site.
pub const MOST_TRIED: usize = 3;

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

/// Whether `host` is webmail, a link shortener or a site that hosts pages
/// for others (or one of their subdomains).
fn hosts_others(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    NOT_ITS_OWN
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// A site a posting's own words point to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its origin.
    pub site: Url,
    /// A link or an email address.
    pub source: SiteSource,
}

/// The sites `text` (a posting) points to, each once, links first and then
/// email addresses, in the order the text names them; never a job board, a
/// tracking system, one of `boards`, webmail, a link shortener or a site
/// that hosts pages for others.
pub fn in_posting(text: &str, boards: &[String]) -> Vec<Found> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || "<>\"'()[]{}|,;".contains(c))
        .map(|w| w.trim_end_matches(['.', ':', '!', '?']))
        .filter(|w| !w.is_empty())
        .collect();
    let links = words
        .iter()
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            lower.starts_with("https://") || lower.starts_with("http://")
        })
        .map(|w| (w.to_string(), SiteSource::PostingLink));
    let emails = words.iter().filter_map(|w| {
        let w: &str = w;
        let w = w.strip_prefix("mailto:").unwrap_or(w);
        let (local, host) = w.split_once('@')?;
        let host = host.split(['?', '/']).next().unwrap_or_default();
        let ok = !local.is_empty()
            && host.contains('.')
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
        ok.then(|| (format!("https://{host}/"), SiteSource::PostingEmail))
    });
    let mut found: Vec<Found> = Vec::new();
    for (link, source) in links.chain(emails) {
        let Some(site) = from_link(&link, boards) else {
            continue;
        };
        if site.host_str().is_none_or(hosts_others) || found.iter().any(|f| f.site == site) {
            continue;
        }
        found.push(Found { site, source });
    }
    found
}

/// The lower-case words of `text` (letters and digits).
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A company name's words: all of them, and the ones a domain would hold
/// (without Inc, Software, Labs…; all of them when that leaves none).
fn name_words(name: &str) -> (Vec<String>, Vec<String>) {
    let all = words_of(name);
    let core: Vec<String> = all
        .iter()
        .filter(|w| !NAME_WORDS_LEFT_OUT.contains(&w.as_str()))
        .cloned()
        .collect();
    let core = if core.is_empty() { all.clone() } else { core };
    (all, core)
}

/// Letters and digits that can be in a domain, joined.
fn domain_key(words: &[String]) -> String {
    words
        .concat()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// Whether `site`'s host is the company `name`'s: one of its labels is the
/// name (a short name), or holds it (a name of six letters or more).
pub fn host_names(site: &Url, name: &str) -> bool {
    let (all, core) = name_words(name);
    let (all, core) = (domain_key(&all), domain_key(&core));
    if core.len() < 2 {
        return false;
    }
    let host = site.host_str().unwrap_or_default().to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    labels.iter().any(|label| {
        let label: String = label.chars().filter(char::is_ascii_alphanumeric).collect();
        label == core || label == all || (core.len() >= 6 && label.contains(&core))
    })
}

/// Domains made from the company's `name`, most likely first: its name
/// without Inc, Software… then all of it, as `.com`, then the short one as
/// `.io`. None for a name too short to be a domain on its own.
pub fn guesses(name: &str) -> Vec<Url> {
    let (all, core) = name_words(name);
    let (all, core) = (domain_key(&all), domain_key(&core));
    if core.len() < 3 {
        return Vec::new();
    }
    let mut hosts = vec![
        format!("{core}.com"),
        format!("{all}.com"),
        format!("{core}.io"),
    ];
    hosts.dedup();
    let mut seen: Vec<String> = Vec::new();
    hosts.retain(|h| {
        // A label is at most 63 characters, plus `.com`.
        let fresh = !seen.contains(h) && h.len() <= 67;
        seen.push(h.clone());
        fresh
    });
    hosts
        .into_iter()
        .filter_map(|host| Url::parse(&format!("https://{host}/")).ok())
        .collect()
}

/// Whether `text` (a home page) names the company `name`: its words in a
/// row, with or without Inc, Software…; or, for a name of six letters or
/// more, the name written as one word (`PerryStreet`).
pub fn names(text: &str, name: &str) -> bool {
    let (all, core) = name_words(name);
    if core.is_empty() {
        return false;
    }
    let spaced = format!(" {} ", words_of(text).join(" "));
    let in_a_row = |words: &[String]| spaced.contains(&format!(" {} ", words.join(" ")));
    let squashed: String = core.concat();
    in_a_row(&all)
        || in_a_row(&core)
        || (squashed.chars().count() >= 6 && words_of(text).concat().contains(&squashed))
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
/// (`own_link`), its own address counts as the apply link; else a link or an
/// email address in the posting's `text` whose host is the company `name`'s.
/// `boards` are the hosts of the job boards the posting was seen on.
#[allow(clippy::too_many_arguments)]
pub fn choose(
    company: Option<&Company>,
    apply_url: Option<&str>,
    posting_url: &str,
    own_link: bool,
    boards: &[String],
    text: Option<&str>,
    name: &str,
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
    if let Some(site) = guess {
        return Choice {
            site: Some(site),
            source: Some(SiteSource::ApplyHost),
        };
    }
    match in_posting(text.unwrap_or_default(), boards)
        .into_iter()
        .find(|found| host_names(&found.site, name))
    {
        Some(found) => Choice {
            site: Some(found.site),
            source: Some(found.source),
        },
        None => Choice {
            site: None,
            source: None,
        },
    }
}

/// The home pages to ask, in order, when [`choose`] found nothing: the
/// posting's own domains that do not spell the name, then domains made from
/// the name; at most [`MOST_TRIED`].
pub fn to_try(text: Option<&str>, boards: &[String], name: &str) -> Vec<Found> {
    let mut tries: Vec<Found> = in_posting(text.unwrap_or_default(), boards);
    for site in guesses(name) {
        if tries.iter().all(|t| t.site != site) {
            tries.push(Found {
                site,
                source: SiteSource::NameGuess,
            });
        }
    }
    tries.truncate(MOST_TRIED);
    tries
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
            read_failure: None,
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
            None,
            "Example Co",
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

    const PERRY: &str = "Senior Ruby Developer\nWe build apps used by millions.\n\
        Questions? Write to jobs@exampleapps.com or see https://boards.greenhouse.io/exampleapps.\n\
        Apply at https://jobs.lever.co/exampleapps/1.";

    fn found(text: &str, name: &str) -> (Option<String>, Option<SiteSource>) {
        let c = choose(
            None,
            Some("https://jobs.lever.co/exampleapps/1"),
            "https://himalayas.app/jobs/x",
            false,
            &["himalayas.app".to_owned()],
            Some(text),
            name,
        );
        (c.site.map(|s| s.to_string()), c.source)
    }

    #[test]
    fn an_email_address_in_the_posting_with_the_companys_name_is_its_site() {
        assert_eq!(
            found(PERRY, "Example Apps Software"),
            (
                Some("https://exampleapps.com/".to_owned()),
                Some(SiteSource::PostingEmail)
            )
        );
    }

    #[test]
    fn a_link_in_the_posting_comes_before_an_email_address() {
        let text = "Read about us at https://careers.exampleapps.com/about. Mail hr@exampleapps.io";
        assert_eq!(
            found(text, "ExampleApps"),
            (
                Some("https://exampleapps.com/".to_owned()),
                Some(SiteSource::PostingLink)
            )
        );
    }

    #[test]
    fn webmail_trackers_shorteners_and_other_names_are_never_its_site() {
        for text in [
            "Send your CV to example.apps.hiring@gmail.com",
            "Apply: https://jobs.ashbyhq.com/exampleapps/1 or https://bit.ly/exampleapps",
            "Our code: https://github.com/exampleapps",
            // Another company's site.
            "We use https://sample-tools.example and https://www.himalayas.app/c/exampleapps",
        ] {
            assert_eq!(found(text, "Example Apps"), (None, None), "{text}");
        }
    }

    #[test]
    fn a_short_name_must_be_a_whole_label() {
        let site = |s: &str| Url::parse(s).unwrap();
        assert!(host_names(&site("https://box.com/"), "Box"));
        assert!(!host_names(&site("https://dropbox.com/"), "Box"));
        assert!(host_names(
            &site("https://www.perrystreet.example/"),
            "Perry Street Software"
        ));
        assert!(host_names(
            &site("https://exampleappshq.com/"),
            "Example Apps"
        ));
    }

    #[test]
    fn guesses_drop_inc_and_software_and_try_the_whole_name() {
        let hosts: Vec<String> = guesses("Example Apps Software, Inc.")
            .iter()
            .map(|u| u.host_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            hosts,
            [
                "exampleapps.com",
                "exampleappssoftwareinc.com",
                "exampleapps.io"
            ]
        );
        assert_eq!(
            guesses("Acme")
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>(),
            ["https://acme.com/", "https://acme.io/"]
        );
        assert!(guesses("A&B").is_empty());
    }

    #[test]
    fn what_to_try_is_the_postings_other_domains_then_the_guesses_three_at_most() {
        let tries = to_try(Some("Mail us at talent@ea-hq.example"), &[], "Example Apps");
        let tried: Vec<(String, SiteSource)> = tries
            .iter()
            .map(|t| (t.site.to_string(), t.source))
            .collect();
        assert_eq!(
            tried,
            [
                (
                    "https://ea-hq.example/".to_owned(),
                    SiteSource::PostingEmail
                ),
                ("https://exampleapps.com/".to_owned(), SiteSource::NameGuess),
                ("https://exampleapps.io/".to_owned(), SiteSource::NameGuess),
            ]
        );
    }

    #[test]
    fn a_home_page_names_the_company_in_words_or_as_one_word() {
        assert!(names(
            "Welcome to Example Apps, makers of things",
            "Example Apps Software"
        ));
        assert!(names("© 2026 ExampleApps Ltd", "Example Apps Software"));
        assert!(!names("Example of apps we like", "Example Apps Software"));
        assert!(!names("Inbox zero", "Box"));
        assert!(names("Box: share files", "Box"));
    }
}
