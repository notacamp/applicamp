//! Finding a company's job board without a model.
//!
//! 1. If a posting of the company already came from a Greenhouse, Lever or
//!    Ashby board, that board is used (the link says it: [`hint_of`]).
//! 2. Otherwise the company's own site is looked at: its home page, the
//!    careers pages it links to and the usual paths (`/careers`, `/jobs`…),
//!    where robots.txt allows, for a Greenhouse, Lever or Ashby board (a link,
//!    an embed or a token) or schema.org `JobPosting` JSON-LD.
//! 3. A board found is read once to confirm it lists jobs.
//!
//! Every page goes through [`Reader::page`] (a public address, the same-site
//! redirect policy, a size cap, a pace); the boards' APIs through
//! [`Reader::board`] at the configured bases, with a checked token. A pasted
//! link to a Greenhouse, Lever or Ashby board is never fetched: its token is
//! read from the link and the API asked.

use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;
use url::Url;

use crate::{
    SourcesDeps,
    job::{Job, Source},
    job_boards::BoardConfig,
    pages::{self, Rules},
    readers::{
        Board, FetchError, Reader, is_token, jsonld,
        mapping::{self, BoardRef},
        posting_link,
    },
};

/// The slug under which jobs read from a company's own careers page (not an
/// applicant tracking system) are named.
pub const CAREERS: &str = "careers";

/// Most bytes read from one careers page.
const MAX_PAGE: usize = 512 * 1024;
/// Most pages read looking for a board, the home page included.
const MAX_PAGES: usize = 6;

/// What a company's jobs are read from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[schema(as = WatchBoardKind)]
pub enum BoardKind {
    Greenhouse,
    Lever,
    Ashby,
    /// A careers page with schema.org `JobPosting` data.
    Jsonld,
    /// No board found.
    None,
}

impl BoardKind {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever => "lever",
            Self::Ashby => "ashby",
            Self::Jsonld => "jsonld",
            Self::None => "none",
        }
    }

    /// The kind stored as `raw`; `None` (no board) for anything else.
    pub fn parse(raw: &str) -> Self {
        [Self::Greenhouse, Self::Lever, Self::Ashby, Self::Jsonld]
            .into_iter()
            .find(|k| k.as_str() == raw)
            .unwrap_or(Self::None)
    }

    /// The reader's source for an applicant tracking system.
    pub const fn source(self) -> Option<Source> {
        match self {
            Self::Greenhouse => Some(Source::Greenhouse),
            Self::Lever => Some(Source::Lever),
            Self::Ashby => Some(Source::Ashby),
            Self::Jsonld | Self::None => None,
        }
    }

    /// The kind of an applicant tracking system's source.
    pub const fn of_source(source: Source) -> Self {
        match source {
            Source::Greenhouse => Self::Greenhouse,
            Source::Lever => Self::Lever,
            Source::Ashby => Self::Ashby,
            Source::Arbeitnow | Source::Hn | Source::Board => Self::None,
        }
    }
}

/// An applicant tracking system's board, found in a page or a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtsBoard {
    pub kind: BoardKind,
    pub token: String,
    /// A Lever board on its EU instance.
    pub eu: bool,
}

/// Words in a link's path that are never a board's token.
const NOT_TOKENS: [&str; 12] = [
    "embed",
    "api",
    "v0",
    "v1",
    "jobs",
    "static",
    "assets",
    "js",
    "css",
    "posting-api",
    "job-board",
    "job_board",
];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
}

static GREENHOUSE_EMBED: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)greenhouse\.io/embed/job_(?:board|app)(?:/js)?\?[^\s\x22'<>]*?\bfor=([A-Za-z0-9][A-Za-z0-9_.-]{0,99})",
    )
});
static GREENHOUSE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:boards|job-boards)(?:\.eu)?\.greenhouse\.io/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})")
});
static GREENHOUSE_API: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)boards-api\.greenhouse\.io/v1/boards/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})")
});
static LEVER: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\bjobs\.(eu\.)?lever\.co/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})"));
static LEVER_API: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\bapi\.(eu\.)?lever\.co/v0/postings/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})")
});
static ASHBY: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\bjobs\.ashbyhq\.com/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})"));
static ASHBY_API: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\bapi\.ashbyhq\.com/posting-api/job-board/([A-Za-z0-9][A-Za-z0-9_.-]{0,99})")
});

/// The Greenhouse, Lever and Ashby boards `text` (a page, or one link)
/// names, in the order it names them, each once.
pub fn ats_in(text: &str) -> Vec<AtsBoard> {
    let mut found: Vec<(usize, AtsBoard)> = Vec::new();
    let mut add = |position: usize, kind: BoardKind, token: &str, eu: bool| {
        let token = token.trim_end_matches(['.', '-']);
        if !is_token(token) || NOT_TOKENS.contains(&token.to_ascii_lowercase().as_str()) {
            return;
        }
        // Greenhouse and Ashby tokens are case-insensitive; Lever's and the
        // others' are used as written.
        found.push((
            position,
            AtsBoard {
                kind,
                token: token.to_owned(),
                eu,
            },
        ));
    };
    for caps in GREENHOUSE_EMBED.captures_iter(text) {
        if let (Some(all), Some(token)) = (caps.get(0), caps.get(1)) {
            add(all.start(), BoardKind::Greenhouse, token.as_str(), false);
        }
    }
    for regex in [&*GREENHOUSE, &*GREENHOUSE_API] {
        for caps in regex.captures_iter(text) {
            if let (Some(all), Some(token)) = (caps.get(0), caps.get(1)) {
                add(all.start(), BoardKind::Greenhouse, token.as_str(), false);
            }
        }
    }
    for regex in [&*LEVER, &*LEVER_API] {
        for caps in regex.captures_iter(text) {
            if let (Some(all), Some(token)) = (caps.get(0), caps.get(2)) {
                add(
                    all.start(),
                    BoardKind::Lever,
                    token.as_str(),
                    caps.get(1).is_some(),
                );
            }
        }
    }
    for regex in [&*ASHBY, &*ASHBY_API] {
        for caps in regex.captures_iter(text) {
            if let (Some(all), Some(token)) = (caps.get(0), caps.get(1)) {
                add(all.start(), BoardKind::Ashby, token.as_str(), false);
            }
        }
    }
    found.sort_by_key(|(position, _)| *position);
    let mut seen = HashSet::new();
    found
        .into_iter()
        .map(|(_, board)| board)
        .filter(|board| seen.insert((board.kind, board.token.to_ascii_lowercase(), board.eu)))
        .collect()
}

/// The board a posting's link says it came from (a Greenhouse, Lever or
/// Ashby posting page).
pub fn hint_of(links: &[&str]) -> Option<AtsBoard> {
    links.iter().find_map(|raw| {
        let url = Url::parse(raw).ok()?;
        let (source, token, _, eu) = posting_link(&url)?;
        Some(AtsBoard {
            kind: BoardKind::of_source(source),
            token,
            eu,
        })
    })
}

/// What looking for a board came to.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// A board that lists jobs (maybe none open).
    Found(Detected),
    /// Nothing was found.
    Nothing(Why),
}

/// Why no board was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The site's robots.txt forbids its pages and nothing else was found.
    RobotsDisallows,
    /// The site could not be reached.
    Unreachable,
    /// The pages read hold no board.
    NoBoard,
}

impl Why {
    /// As stored and sent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RobotsDisallows => "robots_disallows",
            Self::Unreachable => "unreachable",
            Self::NoBoard => "no_board",
        }
    }
}

/// A board found and read.
#[derive(Debug, Clone)]
pub struct Detected {
    pub kind: BoardKind,
    pub token: Option<String>,
    pub eu: bool,
    /// The page that lists the jobs (a careers page), when there is one.
    pub careers_url: Option<Url>,
    /// The company's name, as the page or the board says it.
    pub name: Option<String>,
    /// The jobs the board listed.
    pub jobs: Vec<Job>,
}

/// Where to start looking.
#[derive(Debug, Clone)]
pub enum Start {
    /// The company's own site.
    Site(Url),
    /// A careers page (or a board's address) someone pasted.
    Careers(Url),
    /// A board already known (a posting came from it): only confirm it.
    Board(AtsBoard),
}

/// The company being looked at: its id and name, to name the jobs read.
#[derive(Debug, Clone)]
pub struct Who {
    pub id: uuid::Uuid,
    pub name: String,
}

fn board_of(who: &Who, ats: &AtsBoard) -> Option<Board> {
    Some(Board {
        kind: ats.kind.source()?,
        token: ats.token.clone(),
        company: who.name.clone(),
        eu: ats.eu,
    })
}

/// Read `ats` once. `None` when the board does not answer or is no board.
async fn confirm_ats(
    reader: &Reader,
    db: &sqlx::PgPool,
    who: &Who,
    ats: &AtsBoard,
) -> Option<Vec<Job>> {
    let board = board_of(who, ats)?;
    match reader.board(db, &board).await {
        Ok(jobs) => Some(jobs),
        Err(e) => {
            tracing::info!(kind = ats.kind.as_str(), error = %e, "a company board did not answer");
            None
        }
    }
}

/// The jobs of a careers page's JSON-LD `JobPosting`s, named after the
/// company; those without their own link are told apart by title.
pub fn jsonld_jobs(html: &str, page: &Url, who: &Who) -> Vec<Job> {
    let fields = jsonld::config_with_fields(&BoardConfig::default());
    let board = BoardRef {
        id: who.id,
        slug: CAREERS.to_owned(),
        name: format!("{} careers", who.name),
        drop_expired: true,
    };
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for posting in jsonld::postings(html, page) {
        let Some(mut job) = mapping::job_of(&posting, &fields, &board) else {
            continue;
        };
        if job.company.trim().is_empty() {
            job.company.clone_from(&who.name);
        }
        if !seen.insert(job.source_ref.clone()) {
            // Several postings on one page share its link: tell them apart.
            let slug = crate::job_boards::slug_of(&job.title).unwrap_or_default();
            job.source_ref = format!("{}#{slug}", job.source_ref);
            if !seen.insert(job.source_ref.clone()) {
                continue;
            }
        }
        jobs.push(job);
    }
    jobs
}

/// A name for the company, from a page: its `og:site_name`, else the part of
/// its title before a separator.
pub fn name_of_page(html: &str) -> Option<String> {
    static SITE_NAME: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r#"(?is)<meta[^>]+property\s*=\s*["']og:site_name["'][^>]*content\s*=\s*["']([^"']{1,100})["']"#,
        )
    });
    static TITLE: LazyLock<Regex> = LazyLock::new(|| re(r"(?is)<title[^>]*>(.*?)</title>"));
    if let Some(caps) = SITE_NAME.captures(html)
        && let Some(name) = caps.get(1)
    {
        let name = crate::job::decode_entities(name.as_str());
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_owned());
        }
    }
    let title = TITLE.captures(html)?.get(1)?.as_str();
    let title = crate::job::decode_entities(title);
    let name = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let first = name
        .split(['|', '–', '—', ':', '·'])
        .next()?
        .split(" - ")
        .next()?
        .trim();
    (!first.is_empty() && first.chars().count() <= 100).then(|| first.to_owned())
}

/// A name from a site's host (`careers.example-co.com` → `Example Co`).
pub fn name_of_host(host: &str) -> String {
    let host = host.trim_start_matches("www.");
    let labels: Vec<&str> = host.split('.').collect();
    let main = if labels.len() >= 2 {
        let skip = usize::from(
            labels.len() > 2 && ["careers", "jobs", "join", "work", "apply"].contains(&labels[0]),
        );
        labels.get(skip).copied().unwrap_or(labels[0])
    } else {
        labels[0]
    };
    main.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words in a link's path or text that point at a careers page.
const CAREER_TOPICS: [&str; 7] = [
    "career",
    "jobs",
    "join",
    "hiring",
    "work-with",
    "work with",
    "openings",
];

/// Paths of a careers page a site usually has.
const CAREER_PATHS: [&str; 4] = ["/careers", "/jobs", "/join-us", "/work-with-us"];

/// What one page holds.
enum Page {
    /// An applicant tracking system's board, named by a link or an embed.
    Ats(Vec<AtsBoard>),
    /// JSON-LD `JobPosting`s.
    Postings,
    Nothing,
}

fn scan(html: &str, url: &Url) -> Page {
    let boards = ats_in(html);
    if !boards.is_empty() {
        return Page::Ats(boards);
    }
    if !jsonld::postings(html, url).is_empty() {
        return Page::Postings;
    }
    Page::Nothing
}

/// Look for `who`'s board from `start`, and read it once.
pub async fn find(deps: &SourcesDeps, db: &sqlx::PgPool, who: &Who, start: Start) -> Outcome {
    let Some(reader) = &deps.reader else {
        return Outcome::Nothing(Why::Unreachable);
    };
    match start {
        Start::Board(ats) => match confirm_ats(reader, db, who, &ats).await {
            Some(jobs) => Outcome::Found(Detected {
                kind: ats.kind,
                token: Some(ats.token),
                eu: ats.eu,
                careers_url: None,
                name: None,
                jobs,
            }),
            None => Outcome::Nothing(Why::NoBoard),
        },
        Start::Careers(url) => {
            // A link to a board is read as the board it names, never fetched.
            for ats in ats_in(url.as_str()) {
                if let Some(jobs) = confirm_ats(reader, db, who, &ats).await {
                    return Outcome::Found(Detected {
                        kind: ats.kind,
                        token: Some(ats.token),
                        eu: ats.eu,
                        careers_url: Some(url),
                        name: None,
                        jobs,
                    });
                }
            }
            let mut looked = Looked::new(reader);
            let found = look(reader, db, who, &url, true, &mut looked).await;
            match found {
                Some(found) => Outcome::Found(found),
                None => {
                    // The pasted page was not it: try the company's site.
                    let mut origin = url.clone();
                    origin.set_path("/");
                    origin.set_query(None);
                    origin.set_fragment(None);
                    if let Some(found) = site(reader, db, who, &origin, &mut looked).await {
                        return Outcome::Found(found);
                    }
                    Outcome::Nothing(looked.why())
                }
            }
        }
        Start::Site(origin) => {
            let mut looked = Looked::new(reader);
            match site(reader, db, who, &origin, &mut looked).await {
                Some(found) => Outcome::Found(found),
                None => Outcome::Nothing(looked.why()),
            }
        }
    }
}

/// What looking found out so far.
struct Looked<'a> {
    /// The sites' robots rules, asked once each.
    rules: Rules<'a>,
    /// Pages read.
    read: usize,
    /// Pages robots.txt forbade.
    forbidden: usize,
    /// Pages that could not be reached.
    unreachable: usize,
    /// Pages already looked at.
    seen: HashSet<String>,
}

impl<'a> Looked<'a> {
    fn new(reader: &'a Reader) -> Self {
        Self {
            rules: Rules::new(reader),
            read: 0,
            forbidden: 0,
            unreachable: 0,
            seen: HashSet::new(),
        }
    }

    fn why(&self) -> Why {
        if self.read == 0 && self.forbidden > 0 {
            Why::RobotsDisallows
        } else if self.read == 0 && self.unreachable > 0 {
            Why::Unreachable
        } else {
            Why::NoBoard
        }
    }
}

/// The company's own site: its home page, the careers pages it links to, and
/// the usual paths.
async fn site(
    reader: &Reader,
    db: &sqlx::PgPool,
    who: &Who,
    origin: &Url,
    looked: &mut Looked<'_>,
) -> Option<Detected> {
    let mut home_html = String::new();
    let mut home_url = origin.clone();
    if looked.seen.insert(origin.to_string()) {
        match looked.rules.allows(origin).await {
            Ok(true) => match reader.page(origin, MAX_PAGE).await {
                Ok(Some(page)) => {
                    looked.read += 1;
                    home_html = String::from_utf8_lossy(&page.body).into_owned();
                    home_url = page.url;
                }
                Ok(None) | Err(_) => looked.unreachable += 1,
            },
            Ok(false) => looked.forbidden += 1,
            Err(_) => looked.unreachable += 1,
        }
    }
    if !home_html.is_empty()
        && let Some(found) = act_on(reader, db, who, &home_url, &home_html, false).await
    {
        return Some(found);
    }
    let mut candidates: Vec<Url> = pages::linked_pages_on(&home_html, &home_url, &CAREER_TOPICS);
    for path in CAREER_PATHS {
        if let Ok(url) = origin.join(path) {
            candidates.push(url);
        }
    }
    let mut tried = 0;
    for candidate in candidates {
        if tried + 1 >= MAX_PAGES {
            break;
        }
        if !looked.seen.insert(candidate.to_string()) {
            continue;
        }
        tried += 1;
        if let Some(found) = look(reader, db, who, &candidate, false, looked).await {
            return Some(found);
        }
    }
    None
}

/// One page: read it where robots.txt allows, and act on what it holds.
async fn look(
    reader: &Reader,
    db: &sqlx::PgPool,
    who: &Who,
    url: &Url,
    pasted: bool,
    looked: &mut Looked<'_>,
) -> Option<Detected> {
    looked.seen.insert(url.to_string());
    match looked.rules.allows(url).await {
        Ok(true) => {}
        Ok(false) => {
            looked.forbidden += 1;
            return None;
        }
        Err(_) => {
            looked.unreachable += 1;
            return None;
        }
    }
    let page = match reader.page(url, MAX_PAGE).await {
        Ok(Some(page)) => page,
        Ok(None) => return None,
        Err(FetchError::Refused | FetchError::Transport | FetchError::Status(_)) => {
            looked.unreachable += 1;
            return None;
        }
        Err(_) => return None,
    };
    looked.read += 1;
    let html = String::from_utf8_lossy(&page.body).into_owned();
    act_on(reader, db, who, &page.url, &html, pasted).await
}

/// What a page holds, confirmed by reading the board.
async fn act_on(
    reader: &Reader,
    db: &sqlx::PgPool,
    who: &Who,
    url: &Url,
    html: &str,
    pasted: bool,
) -> Option<Detected> {
    let name = name_of_page(html).filter(|_| pasted);
    match scan(html, url) {
        Page::Ats(boards) => {
            for ats in boards {
                if let Some(jobs) = confirm_ats(reader, db, who, &ats).await {
                    return Some(Detected {
                        kind: ats.kind,
                        token: Some(ats.token),
                        eu: ats.eu,
                        careers_url: Some(url.clone()),
                        name,
                        jobs,
                    });
                }
            }
            None
        }
        Page::Postings => {
            let jobs = jsonld_jobs(html, url, who);
            (!jobs.is_empty()).then(|| Detected {
                kind: BoardKind::Jsonld,
                token: None,
                eu: false,
                careers_url: Some(url.clone()),
                name,
                jobs,
            })
        }
        Page::Nothing => None,
    }
}

/// Read a company's careers page again, as the daily check does: where
/// robots.txt allows, its JSON-LD postings.
pub async fn read_careers(reader: &Reader, who: &Who, url: &Url) -> Result<Vec<Job>, FetchError> {
    let mut rules = Rules::new(reader);
    if !rules.allows(url).await? {
        return Err(FetchError::Refused);
    }
    let page = reader
        .page(url, MAX_PAGE)
        .await?
        .ok_or(FetchError::Status(404))?;
    let html = String::from_utf8_lossy(&page.body);
    let jobs = jsonld_jobs(&html, &page.url, who);
    if jobs.is_empty() && !jsonld::has_postings(&html) {
        return Err(FetchError::Unexpected);
    }
    Ok(jobs)
}

/// Whether `host` is one a company's own site is never: a job board, a
/// professional network or a hiring platform (ADR 0006), whose pages are not
/// a company's.
pub fn is_not_a_company_site(host: &str) -> bool {
    const HOSTS: [&str; 16] = [
        "linkedin.com",
        "indeed.com",
        "glassdoor.com",
        "glassdoor.co.uk",
        "greenhouse.io",
        "lever.co",
        "ashbyhq.com",
        "workable.com",
        "smartrecruiters.com",
        "recruitee.com",
        "teamtailor.com",
        "breezy.hr",
        "bamboohr.com",
        "myworkdayjobs.com",
        "ziprecruiter.com",
        "monster.com",
    ];
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    HOSTS
        .iter()
        .any(|blocked| host == *blocked || host.ends_with(&format!(".{blocked}")))
}

/// The key a company's website is known by: its host without `www.`.
pub fn site_key(url: &Url) -> Option<String> {
    let host = crate::job_boards::host_of(url)?;
    let host = host
        .strip_prefix("careers.")
        .or_else(|| host.strip_prefix("jobs."))
        .unwrap_or(&host)
        .to_owned();
    Some(format!("site:{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(boards: &[AtsBoard]) -> Vec<(BoardKind, String, bool)> {
        boards
            .iter()
            .map(|b| (b.kind, b.token.clone(), b.eu))
            .collect()
    }

    #[test]
    fn links_embeds_and_tokens_name_a_board() {
        let html = r#"
            <a href="https://boards.greenhouse.io/northwind">Open roles</a>
            <script src="https://boards.greenhouse.io/embed/job_board/js?for=contoso"></script>
            <a href="https://jobs.eu.lever.co/fabrikam/abc">Apply</a>
            <a href="https://jobs.ashbyhq.com/tailspin">Careers</a>
            <a href="https://boards-api.greenhouse.io/v1/boards/northwind/jobs">API</a>"#;
        assert_eq!(
            token(&ats_in(html)),
            [
                (BoardKind::Greenhouse, "northwind".to_owned(), false),
                (BoardKind::Greenhouse, "contoso".to_owned(), false),
                (BoardKind::Lever, "fabrikam".to_owned(), true),
                (BoardKind::Ashby, "tailspin".to_owned(), false),
            ]
        );
        assert!(ats_in("<a href=\"https://example.com/careers\">x</a>").is_empty());
        assert!(ats_in("https://boards.greenhouse.io/embed/job_board?for=").is_empty());
    }

    #[test]
    fn a_posting_link_is_a_hint() {
        let hint = hint_of(&[
            "https://example.com/x",
            "https://job-boards.greenhouse.io/northwind/jobs/4001",
        ]);
        assert_eq!(
            hint,
            Some(AtsBoard {
                kind: BoardKind::Greenhouse,
                token: "northwind".into(),
                eu: false
            })
        );
        assert_eq!(hint_of(&["https://example.com/jobs/1"]), None);
    }

    #[test]
    fn names_come_from_the_page_or_the_host() {
        assert_eq!(
            name_of_page(
                r#"<meta property="og:site_name" content="Northwind Stays"><title>x</title>"#
            )
            .as_deref(),
            Some("Northwind Stays")
        );
        assert_eq!(
            name_of_page("<title>Northwind Stays | Careers</title>").as_deref(),
            Some("Northwind Stays")
        );
        assert_eq!(
            name_of_host("careers.northwind-stays.test"),
            "Northwind Stays"
        );
        assert_eq!(name_of_host("www.contoso.example"), "Contoso");
    }

    #[test]
    fn job_boards_and_networks_are_not_company_sites() {
        for host in [
            "www.linkedin.com",
            "indeed.com",
            "uk.indeed.com",
            "boards.greenhouse.io",
            "glassdoor.com",
        ] {
            assert!(is_not_a_company_site(host), "{host}");
        }
        assert!(!is_not_a_company_site("northwind.test"));
        assert!(!is_not_a_company_site("notindeed.com"));
    }

    #[test]
    fn a_site_is_known_by_its_host() {
        let key = |raw: &str| site_key(&raw.parse().unwrap());
        assert_eq!(
            key("https://www.northwind.test/about").as_deref(),
            Some("site:northwind.test")
        );
        assert_eq!(
            key("https://careers.northwind.test/").as_deref(),
            Some("site:northwind.test")
        );
    }
}
