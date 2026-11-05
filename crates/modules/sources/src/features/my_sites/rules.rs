//! What a job site offers, which group it belongs in, and whether it fits a
//! person's goals: pure functions, no database.
//!
//! - **The offer** is read from the site's tags (the kinds of jobs, where,
//!   how remote, its specialities: [`crate::terms`]) and its country. A site
//!   whose tags were never read is taken from its priority: remote-first
//!   means remote jobs open worldwide.
//! - **The group** is the first that fits: a site built around a
//!   speciality or a narrow trade is By trade; a startup list is Startups
//!   and all-jobs sites; a site in the person's country is Your country; a
//!   European site is Europe; a remote-first one open worldwide is Remote,
//!   worldwide; the rest are all-jobs sites.
//! - **The fit** is the app's default for a site (On or Off) from the
//!   person's goals: where they want to work and whether remote, where they
//!   live, and their roles and skills against the site's kinds and
//!   specialities. A person's own choice is never decided here.

use std::collections::HashSet;

use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    job::WorkMode,
    matching::{self, MatchInput},
    places, regions,
    requirements::Reach,
    terms::KINDS,
};

/// The groups a person's list is in, in the order it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SiteGroup)]
pub enum Group {
    /// Remote-first sites open to anyone.
    RemoteWorldwide,
    /// Sites with jobs in Europe.
    Europe,
    /// Sites with jobs in the country the person lives in.
    Country,
    /// Sites for one trade or one stack.
    Trade,
    /// Startup lists and sites with every kind of job.
    Everything,
}

impl Group {
    /// The order they are shown in.
    pub const ALL: [Self; 5] = [
        Self::RemoteWorldwide,
        Self::Europe,
        Self::Country,
        Self::Trade,
        Self::Everything,
    ];
}

/// How remote a site's jobs are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SiteRemote)]
pub enum Remote {
    /// Most of its jobs are remote.
    Mostly,
    /// Some are.
    Some,
    /// Few or none.
    No,
}

/// Where a site's jobs are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Worldwide,
    Europe,
    /// A continent or region (`north-america`).
    Region(String),
    /// One country, ISO 3166-1.
    Country(String),
    Unknown,
}

impl Place {
    /// As the API says it: `worldwide`, `europe`, a region's name or the
    /// country's code; none when unknown.
    pub fn code(&self) -> Option<String> {
        match self {
            Self::Worldwide => Some("worldwide".to_owned()),
            Self::Europe => Some("europe".to_owned()),
            Self::Region(region) => Some(region.clone()),
            Self::Country(code) => Some(code.clone()),
            Self::Unknown => None,
        }
    }
}

/// What a site offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The kinds of jobs it has (empty: every kind, or not known).
    pub kinds: Vec<String>,
    pub place: Place,
    pub remote: Remote,
    /// A stack or a trade it is built around.
    pub specialities: Vec<String>,
    /// It lists startups.
    pub startups: bool,
}

const REGIONS: [&str; 5] = [
    "north-america",
    "latin-america",
    "asia",
    "africa",
    "oceania",
];

/// What a site offers, from its `tags`, its `country` and whether it is
/// remote-first (`high` priority).
pub fn offer_of(tags: &[String], country: Option<&str>, remote_first: bool) -> Offer {
    let mut kinds = Vec::new();
    let mut spots = Vec::new();
    let mut specialities = Vec::new();
    let mut startups = false;
    let mut remote = None;
    for tag in tags {
        let tag = tag.as_str();
        if KINDS.contains(&tag) {
            kinds.push(tag.to_owned());
        } else if tag == "remote" {
            remote = Some(Remote::Mostly);
        } else if tag == "some-remote" {
            remote.get_or_insert(Remote::Some);
        } else if tag == "startups" {
            startups = true;
        } else if tag == "worldwide" {
            spots.push(Place::Worldwide);
        } else if tag == "europe" {
            spots.push(Place::Europe);
        } else if REGIONS.contains(&tag) {
            spots.push(Place::Region(tag.to_owned()));
        } else if let Some(code) = places::countries_in(&tag.replace('-', " ")).first() {
            spots.push(Place::Country((*code).to_owned()));
        } else {
            specialities.push(tag.to_owned());
        }
    }
    let read = !tags.is_empty();
    let place = if let Some(code) = country {
        Place::Country(code.to_owned())
    } else if spots.contains(&Place::Worldwide) {
        Place::Worldwide
    } else if spots.contains(&Place::Europe) {
        Place::Europe
    } else if spots.len() > 1
        && spots
            .iter()
            .all(|p| matches!(p, Place::Country(c) if regions::EUROPEAN.contains(&c.as_str())))
    {
        Place::Europe
    } else if let Some(first) = spots.into_iter().next() {
        first
    } else if !read && remote_first {
        Place::Worldwide
    } else {
        Place::Unknown
    };
    let remote = remote.unwrap_or(if !read && remote_first {
        Remote::Mostly
    } else {
        Remote::No
    });
    Offer {
        kinds,
        place,
        remote,
        specialities,
        startups,
    }
}

/// The group `offer` is in for someone who lives in `home` (ISO 3166-1).
pub fn group_of(offer: &Offer, home: Option<&str>) -> Group {
    let narrow_trade = !offer.kinds.is_empty()
        && offer.kinds.len() <= 2
        && !offer.kinds.iter().any(|k| k == "tech");
    if !offer.specialities.is_empty() || narrow_trade {
        return Group::Trade;
    }
    if offer.startups {
        return Group::Everything;
    }
    match &offer.place {
        Place::Country(code) if Some(code.as_str()) == home => Group::Country,
        Place::Europe => Group::Europe,
        Place::Country(code) if regions::EUROPEAN.contains(&code.as_str()) => Group::Europe,
        Place::Worldwide | Place::Unknown if offer.remote == Remote::Mostly => {
            Group::RemoteWorldwide
        }
        _ => Group::Everything,
    }
}

/// The words of a role that say the kind of work it is.
const KIND_WORDS: [(&str, &[&str]); 22] = [
    (
        "tech",
        &[
            "engineer",
            "engineering",
            "developer",
            "software",
            "programmer",
            "devops",
            "backend",
            "frontend",
            "fullstack",
            "sre",
            "platform",
            "architect",
            "cto",
        ],
    ),
    (
        "data",
        &["data", "analyst", "analytics", "scientist", "machine"],
    ),
    ("design", &["designer", "design", "ux", "creative"]),
    ("product", &["product", "owner"]),
    (
        "marketing",
        &["marketing", "growth", "seo", "content", "brand"],
    ),
    ("sales", &["sales", "account", "sdr", "business"]),
    (
        "finance",
        &[
            "finance",
            "financial",
            "accountant",
            "accounting",
            "controller",
            "auditor",
            "bookkeeper",
        ],
    ),
    ("legal", &["legal", "lawyer", "counsel", "paralegal"]),
    (
        "people",
        &["hr", "recruiter", "recruiting", "talent", "people"],
    ),
    ("operations", &["operations", "ops", "coordinator"]),
    ("support", &["support", "customer", "success", "service"]),
    (
        "health-care",
        &[
            "nurse",
            "doctor",
            "physician",
            "medical",
            "therapist",
            "pharmacist",
            "health",
        ],
    ),
    (
        "education",
        &["teacher", "teaching", "tutor", "professor", "lecturer"],
    ),
    (
        "hospitality-travel",
        &[
            "chef",
            "cook",
            "hotel",
            "hospitality",
            "waiter",
            "bartender",
        ],
    ),
    ("retail", &["retail", "cashier", "store", "shop"]),
    (
        "logistics",
        &["logistics", "driver", "warehouse", "supply", "delivery"],
    ),
    (
        "trades",
        &[
            "electrician",
            "plumber",
            "carpenter",
            "mechanic",
            "welder",
            "technician",
            "construction",
        ],
    ),
    (
        "climate",
        &["climate", "sustainability", "energy", "renewable"],
    ),
    ("nonprofit", &["nonprofit", "charity", "fundraising"]),
    (
        "public-sector",
        &["government", "policy", "civil", "public"],
    ),
    (
        "media",
        &[
            "journalist",
            "editor",
            "writer",
            "video",
            "media",
            "producer",
        ],
    ),
    (
        "science",
        &[
            "researcher",
            "research",
            "biologist",
            "chemist",
            "physicist",
            "laboratory",
        ],
    ),
];

/// What a person's goals say about the sites they want.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wants {
    /// They would take a remote job.
    pub remote: bool,
    /// They would take a job in an office, some or all days.
    pub onsite: bool,
    /// The country they live in.
    pub home: Option<String>,
    /// Europe is a place they can work from (they live there, or would move).
    pub europe: bool,
    /// The kinds of work their roles name; none when no role says.
    pub kinds: Option<HashSet<String>>,
    /// The words of their skills and roles, for a site's specialities.
    pub terms: HashSet<String>,
}

/// What `input` (their profile and goals) says they want.
pub fn wants_of(input: &MatchInput) -> Wants {
    let remote = input.work_modes.is_empty() || input.work_modes.contains(&WorkMode::Remote);
    let onsite = input.work_modes.is_empty()
        || input
            .work_modes
            .iter()
            .any(|m| matches!(m, WorkMode::Hybrid | WorkMode::Onsite));
    let home = input
        .home
        .as_deref()
        .and_then(regions::country_of)
        .or_else(|| {
            input
                .locations
                .iter()
                .find_map(|place| regions::country_of(place))
        })
        .map(str::to_owned);
    let mut countries: Vec<String> = input
        .locations
        .iter()
        .filter_map(|place| regions::country_of(place))
        .map(str::to_owned)
        .collect();
    countries.extend(home.clone());
    let lives_in_europe = countries
        .iter()
        .any(|code| regions::EUROPEAN.contains(&code.as_str()));
    let would_move = !input.will_not_relocate
        && matches!(input.relocate, Some(Reach::WithinEu | Reach::Anywhere));
    let roles: Vec<&String> = if input.roles.is_empty() {
        input.titles.iter().collect()
    } else {
        input.roles.iter().collect()
    };
    let role_words: HashSet<String> = roles.iter().flat_map(|r| matching::words(r)).collect();
    let kinds: HashSet<String> = KIND_WORDS
        .iter()
        .filter(|(_, words)| words.iter().any(|w| role_words.contains(*w)))
        .map(|(kind, _)| (*kind).to_owned())
        .collect();
    let mut terms = role_words;
    for skill in &input.skills {
        terms.extend(matching::words(skill));
    }
    Wants {
        remote,
        onsite,
        home,
        europe: lives_in_europe || would_move,
        kinds: (!kinds.is_empty()).then_some(kinds),
        terms,
    }
}

/// Whether the app turns a site On for someone who `wants` this: the
/// default of the site, `offer` being what it offers and `group` its group
/// for this person.
pub fn fits(offer: &Offer, group: Group, wants: &Wants) -> bool {
    let kind_ok = offer.kinds.is_empty()
        || wants
            .kinds
            .as_ref()
            .is_none_or(|kinds| offer.kinds.iter().any(|k| kinds.contains(k)));
    // Someone who works remote only has no use for a site of office jobs.
    let mode_ok = wants.onsite || offer.remote != Remote::No;
    match group {
        Group::RemoteWorldwide => wants.remote && kind_ok,
        Group::Europe => wants.europe && mode_ok && kind_ok,
        Group::Country => mode_ok && kind_ok,
        Group::Trade => {
            let on_topic = if offer.specialities.is_empty() {
                wants.kinds.is_some() && kind_ok
            } else {
                offer
                    .specialities
                    .iter()
                    .flat_map(|s| matching::words(&s.replace('-', " ")))
                    .any(|w| wants.terms.contains(&w))
            };
            on_topic && mode_ok
        }
        Group::Everything => {
            wants.remote && matches!(offer.place, Place::Worldwide | Place::Unknown) && kind_ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn person(modes: &[WorkMode], home: &str, roles: &[&str], skills: &[&str]) -> Wants {
        wants_of(&MatchInput {
            work_modes: modes.to_vec(),
            home: Some(home.to_owned()),
            roles: roles.iter().map(|s| (*s).to_owned()).collect(),
            skills: skills.iter().map(|s| (*s).to_owned()).collect(),
            ..MatchInput::default()
        })
    }

    fn on(tag_list: &[&str], country: Option<&str>, remote_first: bool, w: &Wants) -> bool {
        let offer = offer_of(&tags(tag_list), country, remote_first);
        let group = group_of(&offer, w.home.as_deref());
        fits(&offer, group, w)
    }

    #[test]
    fn tags_say_what_a_site_offers() {
        let offer = offer_of(
            &tags(&["tech", "worldwide", "remote", "ruby", "rails"]),
            None,
            false,
        );
        assert_eq!(offer.kinds, ["tech"]);
        assert_eq!(offer.place, Place::Worldwide);
        assert_eq!(offer.remote, Remote::Mostly);
        assert_eq!(offer.specialities, ["ruby", "rails"]);

        let offer = offer_of(&tags(&["tech", "portugal", "some-remote"]), None, false);
        assert_eq!(offer.place, Place::Country("PT".to_owned()));
        assert_eq!(offer.remote, Remote::Some);
        assert!(offer.specialities.is_empty());

        // Several European countries make Europe.
        let offer = offer_of(&tags(&["germany", "austria"]), None, false);
        assert_eq!(offer.place, Place::Europe);
        // The registry's country wins.
        let offer = offer_of(&tags(&["tech"]), Some("CH"), false);
        assert_eq!(offer.place, Place::Country("CH".to_owned()));
        // A site whose tags were never read is taken from its priority.
        let offer = offer_of(&[], None, true);
        assert_eq!(offer.place, Place::Worldwide);
        assert_eq!(offer.remote, Remote::Mostly);
        let offer = offer_of(&[], None, false);
        assert_eq!(offer.place, Place::Unknown);
        assert_eq!(offer.remote, Remote::No);
    }

    #[test]
    fn a_site_goes_in_the_first_group_that_fits() {
        let group = |t: &[&str], country: Option<&str>, first: bool| {
            group_of(&offer_of(&tags(t), country, first), Some("PT"))
        };
        assert_eq!(
            group(&["tech", "worldwide", "remote"], None, true),
            Group::RemoteWorldwide
        );
        assert_eq!(group(&[], None, true), Group::RemoteWorldwide);
        assert_eq!(group(&["tech", "europe"], None, false), Group::Europe);
        assert_eq!(group(&["tech"], Some("CH"), false), Group::Europe);
        assert_eq!(group(&["tech"], Some("PT"), false), Group::Country);
        assert_eq!(group(&[], Some("PT"), false), Group::Country);
        assert_eq!(
            group(&["tech", "worldwide", "remote", "elixir"], None, true),
            Group::Trade
        );
        assert_eq!(
            group(&["health-care", "worldwide"], None, false),
            Group::Trade
        );
        assert_eq!(
            group(&["tech", "worldwide", "startups"], None, false),
            Group::Everything
        );
        assert_eq!(group(&["tech"], Some("US"), false), Group::Everything);
        assert_eq!(group(&[], None, false), Group::Everything);
    }

    #[test]
    fn remote_sites_are_on_for_someone_who_works_remote() {
        let remote = person(
            &[WorkMode::Remote],
            "Lisbon",
            &["Staff engineer"],
            &["Rails"],
        );
        assert_eq!(remote.home.as_deref(), Some("PT"));
        assert!(on(&["tech", "worldwide", "remote"], None, true, &remote));
        // Their kind of work only.
        assert!(!on(
            &["design", "worldwide", "remote", "figma"],
            None,
            true,
            &remote
        ));
        assert!(!on(
            &["marketing", "worldwide", "remote"],
            None,
            true,
            &remote
        ));
        // Someone who works on site does not need the remote ones.
        let office = person(&[WorkMode::Onsite], "Lisbon", &["Staff engineer"], &[]);
        assert!(!on(&["tech", "worldwide", "remote"], None, true, &office));
    }

    #[test]
    fn european_and_national_sites_follow_where_they_live() {
        let lisbon = person(&[WorkMode::Remote], "Lisbon", &["Engineer"], &[]);
        assert!(on(&["tech", "europe", "some-remote"], None, false, &lisbon));
        // A remote-only person skips a site of office jobs.
        assert!(!on(&["tech", "europe"], None, false, &lisbon));
        assert!(on(
            &["tech", "portugal", "some-remote"],
            None,
            false,
            &lisbon
        ));
        assert!(!on(&["tech", "portugal"], None, false, &lisbon));
        let mixed = person(
            &[WorkMode::Remote, WorkMode::Hybrid],
            "Lisbon",
            &["Engineer"],
            &[],
        );
        assert!(on(&["tech", "portugal"], None, false, &mixed));
        // Someone in the United States, who will not move, skips Europe.
        let american = person(&[WorkMode::Remote], "New York", &["Engineer"], &[]);
        assert!(!on(
            &["tech", "europe", "some-remote"],
            None,
            false,
            &american
        ));
    }

    #[test]
    fn a_trade_site_is_on_for_the_skills_it_is_built_around() {
        let w = person(
            &[WorkMode::Remote],
            "Lisbon",
            &["Staff engineer"],
            &["Ruby on Rails", "Postgres"],
        );
        assert!(on(
            &["tech", "worldwide", "remote", "ruby", "rails"],
            None,
            true,
            &w
        ));
        assert!(!on(
            &["tech", "worldwide", "remote", "elixir"],
            None,
            true,
            &w
        ));
        assert!(!on(&["tech", "worldwide", "remote", "go"], None, true, &w));
    }

    #[test]
    fn all_jobs_sites_are_on_for_remote_work_in_their_kind() {
        let w = person(&[WorkMode::Remote], "Lisbon", &["Engineer"], &[]);
        assert!(on(&["tech", "worldwide", "startups"], None, false, &w));
        assert!(on(&[], None, false, &w));
        // Jobs in one other country are not worldwide.
        assert!(!on(&["tech", "united-states"], None, false, &w));
        // Nobody asked for remote work.
        let office = person(&[WorkMode::Onsite], "Lisbon", &["Engineer"], &[]);
        assert!(!on(
            &["tech", "worldwide", "startups"],
            None,
            false,
            &office
        ));
    }

    #[test]
    fn without_a_role_every_kind_fits() {
        let w = person(&[], "Lisbon", &[], &[]);
        assert!(w.kinds.is_none());
        assert!(on(&["tech", "worldwide", "remote"], None, true, &w));
    }
}
