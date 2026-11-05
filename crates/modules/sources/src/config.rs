//! Where the job boards are and how searches run, from the environment
//! (`APPLICAMP_SOURCES__…`). Every setting has a default; the API addresses
//! are the boards' own, changed only to point at a fake in tests.
//!
//! | Variable | Default |
//! |---|---|
//! | `APPLICAMP_SOURCES__ENABLED` | every approved job board (and company board kind); else the slugs and kinds listed |
//! | `APPLICAMP_SOURCES__COMPANY_BOARDS` | `false`: the company boards (`boards.toml`) are not searched (kept for #20) |
//! | `APPLICAMP_SOURCES__JOB_BOARDS_FILE` | the job boards built in (`job_boards.toml`) |
//! | `APPLICAMP_SOURCES__ALLOW_PRIVATE_HOSTS` | `false`: job boards are read only at public https hosts |
//! | `APPLICAMP_SOURCES__BOARD_OWNERS` | none: the addresses that may approve, decline, edit and pause boards |
//! | `APPLICAMP_SOURCES__DISCOVERY` | `true` (daily, 04:00 UTC) |
//! | `APPLICAMP_SOURCES__DISCOVERY_LIST_URL` | the CC0 list `emredurukn/awesome-job-boards` |
//! | `APPLICAMP_SOURCES__DISCOVERY_SOURCES` | every list discovery reads (`awesome-job-boards`, `awesome-remote-job`, `remote-working-list`, `awesome-remote-jobs`, `idimetrix-awesome-job-boards`, `wikidata`); else the ones listed |
//! | `APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS` | `100` (boards one discovery run checks, at most) |
//! | `APPLICAMP_SOURCES__GREENHOUSE_URL` | `https://boards-api.greenhouse.io` |
//! | `APPLICAMP_SOURCES__LEVER_URL` | `https://api.lever.co` |
//! | `APPLICAMP_SOURCES__LEVER_EU_URL` | `https://api.eu.lever.co` |
//! | `APPLICAMP_SOURCES__ASHBY_URL` | `https://api.ashbyhq.com` |
//! | `APPLICAMP_SOURCES__WORKABLE_URL` | `https://apply.workable.com` |
//! | `APPLICAMP_SOURCES__RECRUITEE_URL` | `https://{token}.recruitee.com` (`{token}` is the company's) |
//! | `APPLICAMP_SOURCES__TEAMTAILOR_URL` | `https://{token}.teamtailor.com` |
//! | `APPLICAMP_SOURCES__PERSONIO_URL` | `https://{token}.jobs.personio.de` |
//! | `APPLICAMP_SOURCES__HN_URL` | `https://hn.algolia.com` |
//! | `APPLICAMP_SOURCES__COMPANY_PAGES` | `true`: company hiring pages found through apply links are checked every day |
//! | `APPLICAMP_SOURCES__COMPANY_PAGE_CHECKS` | `300` (new company pages one daily run checks, at most) |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST` | `true`: once a day (06:00 UTC) Workable's list of every company's jobs adds the companies that are hiring as company pages |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST_URL` | `https://www.workable.com/boards/workable.xml` |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST_MAX_BYTES` | `1500000000` (the download stops there and keeps what it read; the list is about 860 MB) |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST_TIMEOUT_SECS` | `3600` (the whole download; an idle connection is cut after 60 s) |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST_LOOKUPS` | `500` (companies whose address on Workable is looked up a day, one request each) |
//! | `APPLICAMP_SOURCES__WORKABLE_LIST_EXCLUDE` | none: names or sites of boards Workable already integrates with, never added from the list |
//! | `APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES` | `false`: a search reads only the company pages of companies the person watches |
//! | `APPLICAMP_SEARCH__BRAVE_API_KEY_FILE` | unset: web search is off (a file holding the key) |
//! | `APPLICAMP_SEARCH__BRAVE_URL` | `https://api.search.brave.com` |
//! | `APPLICAMP_SEARCH__BRAVE_WEEKLY_QUERIES` | `50` (queries one weekly run asks, at most) |
//! | `APPLICAMP_SEARCH__BRAVE_MONTHLY_QUERIES` | `1000` (queries a month, at most: Brave bills each) |
//! | `APPLICAMP_SOURCES__BOARDS_FILE` | the company boards built in (`boards.toml`) |
//! | `APPLICAMP_SOURCES__CACHE_SECS` | `21600` (6 hours; `0` never caches; job boards keep to their own polling interval) |
//! | `APPLICAMP_SOURCES__REQUESTS_PER_SECOND` | `2` (per host) |
//! | `APPLICAMP_SOURCES__MANUAL_PER_DAY` | `3` (`0` for no limit) |
//! | `APPLICAMP_SOURCES__MANUAL_EVERY_SECS` | `900` (`0` for no spacing) |
//! | `APPLICAMP_SOURCES__NIGHTLY` | `true` (21:00 Europe/Lisbon) |
//! | `APPLICAMP_SOURCES__WATCH_EVERY_SECS` | `72000` (a watched company's board is read at most this often; `0` reads it in every search) |
//! | `APPLICAMP_SOURCES__FULL_SCORES_PER_SEARCH` | `100` (jobs one search reads in full, at most, the best of its quick looks) |

use std::path::PathBuf;

use applicamp_kernel::config::env_source;
use figment::Figment;
use serde::{Deserialize, Deserializer};
use url::Url;

use crate::job_boards::is_slug;

/// The sources settings.
#[derive(Debug, Clone, Deserialize)]
pub struct SourcesConfig {
    /// The job boards (slugs) and company board kinds read,
    /// comma-separated; unset is every approved one.
    #[serde(default, deserialize_with = "list")]
    pub enabled: Option<Vec<String>>,
    /// Search the company boards (Greenhouse, Lever, Ashby) too.
    #[serde(default, deserialize_with = "flag")]
    pub company_boards: Option<bool>,
    /// A job boards list to use instead of the built-in one.
    #[serde(default)]
    pub job_boards_file: Option<PathBuf>,
    /// Read job boards at any http(s) host, private ones too (tests, a demo
    /// with fake boards).
    #[serde(default, deserialize_with = "flag")]
    pub allow_private_hosts: Option<bool>,
    /// The addresses (lower case) whose accounts manage the job boards.
    #[serde(default, deserialize_with = "list")]
    pub board_owners: Option<Vec<String>>,
    /// Look for new job boards every day.
    #[serde(default, deserialize_with = "flag")]
    pub discovery: Option<bool>,
    /// The list of job boards discovery reads (names and links only).
    #[serde(default = "discovery_list_url")]
    pub discovery_list_url: Url,
    #[serde(default, deserialize_with = "list")]
    pub discovery_sources: Option<Vec<String>>,
    #[serde(default, deserialize_with = "number")]
    pub discovery_max_checks: Option<u64>,
    /// Greenhouse's job board API.
    #[serde(default = "greenhouse_url")]
    pub greenhouse_url: Url,
    /// Lever's postings API.
    #[serde(default = "lever_url")]
    pub lever_url: Url,
    /// Lever's postings API for boards on its EU instance.
    #[serde(default = "lever_eu_url")]
    pub lever_eu_url: Url,
    /// Ashby's posting API.
    #[serde(default = "ashby_url")]
    pub ashby_url: Url,
    /// HN's search API (Algolia).
    #[serde(default = "hn_url")]
    pub hn_url: Url,
    /// A boards list to use instead of the built-in seed list.
    #[serde(default)]
    pub boards_file: Option<PathBuf>,
    /// How long a board's answer is reused, in seconds.
    #[serde(default, deserialize_with = "number")]
    pub cache_secs: Option<u64>,
    /// Requests per second to one host.
    #[serde(default, deserialize_with = "number")]
    pub requests_per_second: Option<u64>,
    /// Manual searches per account per UTC day.
    #[serde(default, deserialize_with = "number")]
    pub manual_per_day: Option<u64>,
    /// Seconds between two manual searches of one account.
    #[serde(default, deserialize_with = "number")]
    pub manual_every_secs: Option<u64>,
    /// Search every night for accounts with goals.
    #[serde(default, deserialize_with = "flag")]
    pub nightly: Option<bool>,
    /// Seconds between two reads of one watched company's job board.
    #[serde(default, deserialize_with = "number")]
    pub watch_every_secs: Option<u64>,
    /// Jobs one search reads in full, at most.
    #[serde(default, deserialize_with = "number")]
    pub full_scores_per_search: Option<u64>,
    /// Workable's widget API.
    #[serde(default = "workable_url")]
    pub workable_url: Url,
    /// Recruitee's offers API: the address of a company's, `{token}` standing
    /// for the company (a fake in tests has none).
    #[serde(default = "recruitee_url")]
    pub recruitee_url: String,
    /// Teamtailor's job feed, the same way.
    #[serde(default = "teamtailor_url")]
    pub teamtailor_url: String,
    /// Personio's XML feed, the same way.
    #[serde(default = "personio_url")]
    pub personio_url: String,
    /// Check the company pages found through apply links every day.
    #[serde(default, deserialize_with = "flag")]
    pub company_pages: Option<bool>,
    /// New company pages one daily run checks, at most.
    #[serde(default, deserialize_with = "number")]
    pub company_page_checks: Option<u64>,
    /// A search reads the company pages of the companies the person watches.
    #[serde(default, deserialize_with = "flag")]
    pub search_company_pages: Option<bool>,
    /// Read Workable's list of every company's jobs once a day.
    #[serde(default, deserialize_with = "flag")]
    pub workable_list: Option<bool>,
    /// Where Workable's list is.
    #[serde(default = "workable_list_url")]
    pub workable_list_url: Url,
    /// The most bytes one download reads.
    #[serde(default, deserialize_with = "number")]
    pub workable_list_max_bytes: Option<u64>,
    /// Seconds one download may take.
    #[serde(default, deserialize_with = "number")]
    pub workable_list_timeout_secs: Option<u64>,
    /// Companies whose Workable address is looked up a day, at most.
    #[serde(default, deserialize_with = "number")]
    pub workable_list_lookups: Option<u64>,
    /// Names or sites of boards Workable already integrates with.
    #[serde(default, deserialize_with = "list")]
    pub workable_list_exclude: Option<Vec<String>>,
    /// The web search (`APPLICAMP_SEARCH__…`).
    #[serde(skip)]
    pub search: SearchConfig,
}

/// The optional web search that finds company pages (`APPLICAMP_SEARCH__…`).
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// A file holding the Brave Search API key; unset is off.
    #[serde(default)]
    pub brave_api_key_file: Option<PathBuf>,
    /// Brave Search's API.
    #[serde(default = "brave_url")]
    pub brave_url: Url,
    /// Queries one weekly run asks, at most.
    #[serde(default, deserialize_with = "number")]
    pub brave_weekly_queries: Option<u64>,
    /// Queries a calendar month, at most.
    #[serde(default, deserialize_with = "number")]
    pub brave_monthly_queries: Option<u64>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            brave_api_key_file: None,
            brave_url: brave_url(),
            brave_weekly_queries: None,
            brave_monthly_queries: None,
        }
    }
}

impl SearchConfig {
    /// Whether the web search is on: a key file is named.
    pub fn brave(&self) -> bool {
        self.brave_api_key_file.is_some()
    }

    /// Queries one weekly run asks, at most.
    pub fn weekly_queries(&self) -> u64 {
        self.brave_weekly_queries.unwrap_or(50).min(1_000)
    }

    /// Queries a calendar month, at most.
    pub fn monthly_queries(&self) -> u64 {
        self.brave_monthly_queries.unwrap_or(1_000).min(100_000)
    }
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            company_boards: None,
            job_boards_file: None,
            allow_private_hosts: None,
            board_owners: None,
            discovery: None,
            discovery_list_url: discovery_list_url(),
            discovery_sources: None,
            discovery_max_checks: None,
            greenhouse_url: greenhouse_url(),
            lever_url: lever_url(),
            lever_eu_url: lever_eu_url(),
            ashby_url: ashby_url(),
            hn_url: hn_url(),
            boards_file: None,
            cache_secs: None,
            requests_per_second: None,
            manual_per_day: None,
            manual_every_secs: None,
            nightly: None,
            watch_every_secs: None,
            full_scores_per_search: None,
            workable_url: workable_url(),
            recruitee_url: recruitee_url(),
            teamtailor_url: teamtailor_url(),
            personio_url: personio_url(),
            company_pages: None,
            company_page_checks: None,
            search_company_pages: None,
            workable_list: None,
            workable_list_url: workable_list_url(),
            workable_list_max_bytes: None,
            workable_list_timeout_secs: None,
            workable_list_lookups: None,
            workable_list_exclude: None,
            search: SearchConfig::default(),
        }
    }
}

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap_or_else(|e| unreachable!("a built-in URL parses: {e}"))
}

fn greenhouse_url() -> Url {
    url("https://boards-api.greenhouse.io")
}
fn lever_url() -> Url {
    url("https://api.lever.co")
}
fn lever_eu_url() -> Url {
    url("https://api.eu.lever.co")
}
fn ashby_url() -> Url {
    url("https://api.ashbyhq.com")
}
fn workable_url() -> Url {
    url("https://apply.workable.com")
}
fn workable_list_url() -> Url {
    url("https://www.workable.com/boards/workable.xml")
}
fn recruitee_url() -> String {
    "https://{token}.recruitee.com".to_owned()
}
fn teamtailor_url() -> String {
    "https://{token}.teamtailor.com".to_owned()
}
fn personio_url() -> String {
    "https://{token}.jobs.personio.de".to_owned()
}
fn brave_url() -> Url {
    url("https://api.search.brave.com")
}
fn discovery_list_url() -> Url {
    url("https://raw.githubusercontent.com/emredurukn/awesome-job-boards/main/README.md")
}
fn hn_url() -> Url {
    url("https://hn.algolia.com")
}

impl SourcesConfig {
    /// The settings in the `APPLICAMP_SOURCES__…` variables.
    pub fn from_env() -> Result<Self, String> {
        Self::from_figment(&env_source())
    }

    /// The `sources` section of `figment`.
    pub fn from_figment(figment: &Figment) -> Result<Self, String> {
        let search: SearchConfig = if figment.contains("search") {
            figment
                .extract_inner("search")
                .map_err(|e| format!("APPLICAMP_SEARCH__…: {e}"))?
        } else {
            SearchConfig::default()
        };
        if !figment.contains("sources") {
            return Ok(Self {
                search,
                ..Self::default()
            });
        }
        let mut config: Self = figment
            .extract_inner("sources")
            .map_err(|e| format!("APPLICAMP_SOURCES__…: {e}"))?;
        config.search = search;
        for (name, template) in [
            ("RECRUITEE_URL", &config.recruitee_url),
            ("TEAMTAILOR_URL", &config.teamtailor_url),
            ("PERSONIO_URL", &config.personio_url),
        ] {
            if Url::parse(&template.replace("{token}", "example")).is_err() {
                return Err(format!(
                    "APPLICAMP_SOURCES__{name}: `{template}` is not an address"
                ));
            }
        }
        if let Some(unknown) = config.enabled.iter().flatten().find(|name| !is_slug(name)) {
            return Err(format!(
                "APPLICAMP_SOURCES__ENABLED: `{unknown}` is not a job board's slug or a company \
                 board kind"
            ));
        }
        if let Some(bad) = config
            .board_owners
            .iter()
            .flatten()
            .find(|email| !email.contains('@'))
        {
            return Err(format!(
                "APPLICAMP_SOURCES__BOARD_OWNERS: `{bad}` is not an address"
            ));
        }
        Ok(config)
    }

    /// Whether the job board `slug` (or the company board kind `slug`) is
    /// read.
    pub fn is_enabled(&self, slug: &str) -> bool {
        self.enabled
            .as_ref()
            .is_none_or(|names| names.iter().any(|name| name == slug))
    }

    /// Whether the company boards are searched (off: kept for watchers, #20).
    pub fn company_boards(&self) -> bool {
        self.company_boards.unwrap_or(false)
    }

    /// Whether the company pages found through apply links are checked every
    /// day.
    pub fn company_pages(&self) -> bool {
        self.company_pages.unwrap_or(true)
    }

    /// New company pages one daily run checks, at most (300 when left out).
    pub fn company_page_checks(&self) -> usize {
        usize::try_from(self.company_page_checks.unwrap_or(300).clamp(1, 10_000)).unwrap_or(300)
    }

    /// Whether Workable's list is read every day.
    pub fn workable_list(&self) -> bool {
        self.workable_list.unwrap_or(true)
    }

    /// The most bytes one download of the list reads (1.5 GB when left out).
    pub fn workable_list_max_bytes(&self) -> u64 {
        self.workable_list_max_bytes
            .unwrap_or(1_500_000_000)
            .clamp(1_000, 16_000_000_000)
    }

    /// How long one download of the list may take (an hour when left out).
    pub fn workable_list_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.workable_list_timeout_secs
                .unwrap_or(3_600)
                .clamp(1, 7_000),
        )
    }

    /// Companies whose address on Workable is looked up in a day, at most.
    pub fn workable_list_lookups(&self) -> usize {
        usize::try_from(self.workable_list_lookups.unwrap_or(500).min(20_000)).unwrap_or(500)
    }

    /// Whether `name` or `site` is on the list of boards Workable already
    /// integrates with (compared in lower case, a site by its host).
    pub fn workable_list_excludes(&self, name: &str, site: Option<&str>) -> bool {
        let host = site.and_then(|raw| {
            Url::parse(raw)
                .ok()
                .or_else(|| Url::parse(&format!("https://{raw}")).ok())
                .and_then(|u| {
                    u.host_str()
                        .map(|h| h.trim_start_matches("www.").to_owned())
                })
        });
        self.workable_list_exclude.iter().flatten().any(|entry| {
            let entry = entry.trim_start_matches("www.");
            entry == name.to_ascii_lowercase().trim()
                || host
                    .as_deref()
                    .is_some_and(|h| h.eq_ignore_ascii_case(entry))
        })
    }

    /// Whether a search reads the company pages of the companies the person
    /// watches (off by default: a job store comes first).
    pub fn search_company_pages(&self) -> bool {
        self.search_company_pages.unwrap_or(false)
    }

    /// The address of a company's page on a system whose address holds
    /// `{token}` (Recruitee, Teamtailor, Personio); `None` for a token that is
    /// not a safe name for a host.
    pub fn company_host(&self, template: &str, token: &str) -> Option<Url> {
        if token.is_empty()
            || token.len() > 63
            || token.starts_with('-')
            || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return None;
        }
        Url::parse(&template.replace("{token}", &token.to_ascii_lowercase())).ok()
    }

    /// Whether job boards may be read at private hosts.
    pub fn allow_private_hosts(&self) -> bool {
        self.allow_private_hosts.unwrap_or(false)
    }

    /// Whether the account with `email` manages the job boards.
    pub fn is_board_owner(&self, email: &str) -> bool {
        let email = email.trim().to_ascii_lowercase();
        !email.is_empty()
            && self
                .board_owners
                .iter()
                .flatten()
                .any(|owner| owner.trim().eq_ignore_ascii_case(&email))
    }

    /// Whether discovery runs every day.
    pub fn discovery(&self) -> bool {
        self.discovery.unwrap_or(true)
    }

    /// Boards one discovery run checks, at most (100 when left out).
    pub fn discovery_max_checks(&self) -> usize {
        usize::try_from(self.discovery_max_checks.unwrap_or(100).clamp(1, 5_000)).unwrap_or(100)
    }

    /// Whether discovery reads the list `id` (every list when left out).
    pub fn reads_list(&self, id: &str) -> bool {
        self.discovery_sources
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|name| name == id))
    }

    /// How long a board's answer is reused.
    pub fn cache_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.cache_secs.unwrap_or(6 * 3600))
    }

    /// The wait between two requests to one host.
    pub fn request_spacing(&self) -> std::time::Duration {
        let per_second = self.requests_per_second.unwrap_or(2).max(1);
        std::time::Duration::from_millis(1_000 / per_second)
    }

    /// Manual searches per account per day; `None` is no limit.
    pub fn manual_per_day(&self) -> Option<u64> {
        Some(self.manual_per_day.unwrap_or(3)).filter(|n| *n > 0)
    }

    /// The least time between two manual searches; `None` is none.
    pub fn manual_every(&self) -> Option<chrono::TimeDelta> {
        let secs = self.manual_every_secs.unwrap_or(15 * 60);
        (secs > 0).then(|| chrono::TimeDelta::seconds(i64::try_from(secs).unwrap_or(i64::MAX)))
    }

    /// Whether the nightly search runs.
    pub fn nightly(&self) -> bool {
        self.nightly.unwrap_or(true)
    }

    /// The least time between two reads of one watched company's job board:
    /// a search (the nightly one, or "Search now") reads a board only when
    /// this has passed since its last read. Twenty hours by default, so the
    /// nightly search reads it once a day and a second search the same day
    /// does not.
    pub fn watch_every(&self) -> chrono::TimeDelta {
        let secs = self.watch_every_secs.unwrap_or(20 * 3600);
        chrono::TimeDelta::seconds(i64::try_from(secs).unwrap_or(i64::MAX))
    }

    /// Jobs one search reads in full, at most: the best of its quick looks
    /// (at least one).
    pub fn full_scores_per_search(&self) -> usize {
        usize::try_from(self.full_scores_per_search.unwrap_or(100).max(1)).unwrap_or(100)
    }
}

/// Comma-separated names; blank is unset.
fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        List(Vec<String>),
        Text(String),
    }
    Ok(match Option::<Raw>::deserialize(d)? {
        None => None,
        Some(Raw::List(items)) => Some(items),
        Some(Raw::Text(text)) if text.trim().is_empty() => None,
        Some(Raw::Text(text)) => Some(
            text.split(',')
                .map(|name| name.trim().to_ascii_lowercase())
                .filter(|name| !name.is_empty())
                .collect(),
        ),
    })
}

/// A whole number, as a number or as text; blank is unset.
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Text(String),
    }
    match Option::<Raw>::deserialize(d)? {
        None => Ok(None),
        Some(Raw::Number(n)) => Ok(Some(n)),
        Some(Raw::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Raw::Text(text)) => text
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| serde::de::Error::custom(format!("`{text}` is not a whole number"))),
    }
}

/// A yes or no, as a boolean or as text; blank is unset.
fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Bool(bool),
        Text(String),
    }
    match Option::<Raw>::deserialize(d)? {
        None => Ok(None),
        Some(Raw::Bool(b)) => Ok(Some(b)),
        Some(Raw::Text(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "" => Ok(None),
            "true" | "1" | "yes" => Ok(Some(true)),
            "false" | "0" | "no" => Ok(Some(false)),
            _ => Err(serde::de::Error::custom(format!(
                "`{text}` is not true or false"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use figment::providers::Serialized;

    use super::*;

    fn load(value: serde_json::Value) -> Result<SourcesConfig, String> {
        SourcesConfig::from_figment(&Figment::from(Serialized::defaults(value)))
    }

    #[test]
    fn defaults_are_the_boards_own_apis_and_the_limits() {
        let config = load(serde_json::json!({})).unwrap();
        assert_eq!(
            config.greenhouse_url.as_str(),
            "https://boards-api.greenhouse.io/"
        );
        assert!(config.is_enabled("hn"));
        assert!(
            !config.company_boards(),
            "company boards are off by default"
        );
        assert!(!config.allow_private_hosts());
        assert!(config.discovery());
        assert!(!config.is_board_owner("owner@example.com"));
        assert_eq!(config.manual_per_day(), Some(3));
        assert_eq!(config.manual_every(), Some(chrono::TimeDelta::minutes(15)));
        assert_eq!(
            config.request_spacing(),
            std::time::Duration::from_millis(500)
        );
        assert!(config.nightly());
        assert_eq!(config.full_scores_per_search(), 100);
    }

    #[test]
    fn settings_are_read_and_zero_lifts_a_limit() {
        let config = load(serde_json::json!({ "sources": {
            "enabled": "greenhouse, Lever, himalayas",
            "company_boards": "true",
            "board_owners": "Owner@Example.com",
            "greenhouse_url": "http://fake-jobs:9020",
            "cache_secs": "0",
            "manual_per_day": 0,
            "manual_every_secs": "0",
            "nightly": "false",
            "full_scores_per_search": "20",
        }}))
        .unwrap();
        assert!(config.is_enabled("greenhouse"));
        assert!(config.is_enabled("lever"));
        assert!(config.is_enabled("himalayas"));
        assert!(!config.is_enabled("hn"));
        assert!(config.company_boards());
        assert!(config.is_board_owner("owner@example.com"));
        assert_eq!(config.greenhouse_url.as_str(), "http://fake-jobs:9020/");
        assert_eq!(config.cache_ttl(), std::time::Duration::ZERO);
        assert_eq!(config.manual_per_day(), None);
        assert_eq!(config.manual_every(), None);
        assert!(!config.nightly());
        assert_eq!(config.full_scores_per_search(), 20);
        let err = load(serde_json::json!({ "sources": { "enabled": "Linked In" } })).unwrap_err();
        assert!(err.contains("linked in"), "{err}");
    }
}
