//! Reading a company's own pages, for the facts a tailored CV is built on.
//!
//! One function, [`read_site`], and nothing else reaches the web for it:
//!
//! - the **posting page** itself (its own host, its own robots.txt),
//! - the company's **home page**,
//! - up to [`MOST_LINKED`] more pages **on the same site**, linked from the
//!   home page, whose path or link text names one of [`TOPICS`].
//!
//! Never another host: no news, no LinkedIn, no review sites, no search
//! engine. Every request goes through the reader's client (public addresses
//! only, a timeout, a pace, no cookies) and only where the site's robots.txt
//! allows us (`robots`); a page it forbids is not read, and is counted.
//! Each page is cut at [`MAX_PAGE`] bytes and turned into plain text. What
//! comes back is untrusted data for a model to quote, never instructions.

use std::{collections::HashMap, sync::LazyLock};

use chrono::{DateTime, Utc};
use regex::Regex;
use url::Url;

use crate::{
    job::html_to_text,
    job_boards::RobotsRule,
    readers::{FetchError, Reader, same_site},
    robots,
};

/// Most bytes read from one page.
pub const MAX_PAGE: usize = 300 * 1024;
/// Most pages read besides the posting and the home page.
pub const MOST_LINKED: usize = 5;
/// What a linked page's path or link text must name, most wanted first.
pub const TOPICS: [&str; 9] = [
    "about",
    "company",
    "customers",
    "clients",
    "product",
    "engineering",
    "team",
    "blog",
    "careers",
];

/// Why a page of a site is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The job's own page.
    Posting,
    /// The company's home page.
    Home,
    /// A page the home page links to.
    Linked,
}

impl Kind {
    /// As stored and sent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Posting => "posting",
            Self::Home => "home",
            Self::Linked => "linked",
        }
    }
}

/// One page read.
#[derive(Debug, Clone)]
pub struct Page {
    /// Where it was read (after a redirect inside the site).
    pub url: Url,
    /// When.
    pub fetched_at: DateTime<Utc>,
    /// Why.
    pub kind: Kind,
    /// Its text.
    pub text: String,
}

/// What reading a site came to.
#[derive(Debug, Clone, Default)]
pub struct SiteRead {
    /// The pages read, the posting first.
    pub pages: Vec<Page>,
    /// Pages left alone because the site's robots.txt forbids us.
    pub blocked: u32,
}

/// Why a site could not be read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The site's address is not one we may ask, or it does not answer.
    Unreachable,
    /// The site's robots.txt forbids its home page (so none of it is read).
    RobotsDisallows,
    /// It answered, but no page had text.
    NothingReadable,
}

/// The robots rules of each host asked in one reading.
struct Rules<'a> {
    reader: &'a Reader,
    by_host: HashMap<String, Vec<RobotsRule>>,
}

impl<'a> Rules<'a> {
    fn new(reader: &'a Reader) -> Self {
        Self {
            reader,
            by_host: HashMap::new(),
        }
    }

    /// Whether we may read `url`. A missing robots.txt allows everything; one
    /// that cannot be fetched at all means we do not know, so we do not read.
    async fn allows(&mut self, url: &Url) -> Result<bool, FetchError> {
        let key = format!(
            "{}:{}",
            url.host_str().unwrap_or_default().to_ascii_lowercase(),
            url.port().map_or(String::new(), |p| p.to_string())
        );
        if !self.by_host.contains_key(&key) {
            let mut robots_url = url.clone();
            robots_url.set_path("/robots.txt");
            robots_url.set_query(None);
            robots_url.set_fragment(None);
            let rules = match self.reader.page(&robots_url, 512 * 1024).await? {
                Some(page) => robots::rules_for_us(&String::from_utf8_lossy(&page.body)).0,
                None => Vec::new(),
            };
            self.by_host.insert(key.clone(), rules);
        }
        let rules = self.by_host.get(&key).map_or(&[][..], Vec::as_slice);
        Ok(robots::allowed(rules, &robots::path_of(url)).0)
    }
}

/// Read `site`'s home page and the pages it links to about the company, and
/// `posting` (the job's own page), where robots.txt allows. The posting is
/// optional: a failure to read it is not a failure of the reading.
pub async fn read_site(
    reader: &Reader,
    site: &Url,
    posting: Option<&Url>,
    now: DateTime<Utc>,
) -> Result<SiteRead, Failure> {
    let mut rules = Rules::new(reader);
    let mut read = SiteRead::default();

    if let Some(posting) = posting {
        match rules.allows(posting).await {
            Ok(true) => {
                if let Ok(Some(page)) = reader.page(posting, MAX_PAGE).await {
                    push(&mut read, page.url, &page.body, Kind::Posting, now);
                }
            }
            Ok(false) => read.blocked += 1,
            Err(_) => {}
        }
    }

    match rules.allows(site).await {
        Ok(true) => {}
        Ok(false) => return Err(Failure::RobotsDisallows),
        Err(_) => return Err(Failure::Unreachable),
    }
    let home = match reader.page(site, MAX_PAGE).await {
        Ok(Some(page)) => page,
        Ok(None) | Err(_) => return Err(Failure::Unreachable),
    };
    let html = String::from_utf8_lossy(&home.body).into_owned();
    let home_url = home.url.clone();
    push(&mut read, home.url, &home.body, Kind::Home, now);

    let mut taken = 0;
    for link in linked_pages(&html, &home_url) {
        if taken == MOST_LINKED {
            break;
        }
        match rules.allows(&link).await {
            Ok(true) => {}
            Ok(false) => {
                read.blocked += 1;
                continue;
            }
            Err(_) => continue,
        }
        if let Ok(Some(page)) = reader.page(&link, MAX_PAGE).await
            && same_site(&page.url, &home_url)
        {
            // A page a redirect brought to one already read adds nothing.
            if read.pages.iter().all(|p| p.url != page.url) {
                push(&mut read, page.url, &page.body, Kind::Linked, now);
                taken += 1;
            }
        }
    }

    if read
        .pages
        .iter()
        .all(|page| page.kind == Kind::Posting || page.text.trim().is_empty())
    {
        return Err(Failure::NothingReadable);
    }
    Ok(read)
}

fn push(read: &mut SiteRead, url: Url, body: &[u8], kind: Kind, now: DateTime<Utc>) {
    let text = html_to_text(&String::from_utf8_lossy(body));
    if text.trim().is_empty() {
        return;
    }
    read.pages.push(Page {
        url,
        fetched_at: now,
        kind,
        text,
    });
}

static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\b[^>]*?\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')[^>]*>(.*?)</a\s*>"#)
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Files that are not pages.
const NOT_PAGES: [&str; 14] = [
    ".pdf", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".zip", ".mp4", ".mp3", ".xml",
    ".json", ".css", ".js",
];

/// The pages `html` (the page at `home`) links to on the same site whose path
/// or link text names a topic, most wanted topic first, then in the order
/// the page lists them; each once, never the home page itself.
pub fn linked_pages(html: &str, home: &Url) -> Vec<Url> {
    let mut found: Vec<(usize, usize, Url)> = Vec::new();
    for (order, caps) in ANCHOR.captures_iter(html).enumerate() {
        let href = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map_or("", |m| m.as_str().trim());
        let label = html_to_text(caps.get(3).map_or("", |m| m.as_str())).to_lowercase();
        if href.is_empty() || href.starts_with('#') {
            continue;
        }
        let Ok(mut url) = home.join(href) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") || !same_site(&url, home) {
            continue;
        }
        url.set_fragment(None);
        let path = url.path().to_ascii_lowercase();
        if path == "/" || path == home.path() || NOT_PAGES.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        let rank = TOPICS
            .iter()
            .position(|topic| path.contains(topic) || label.contains(topic));
        let Some(rank) = rank else {
            continue;
        };
        if found.iter().all(|(_, _, seen)| *seen != url) {
            found.push((rank, order, url));
        }
    }
    found.sort_by_key(|(rank, order, _)| (*rank, *order));
    found.into_iter().map(|(_, _, url)| url).collect()
}

#[cfg(test)]
mod tests;
