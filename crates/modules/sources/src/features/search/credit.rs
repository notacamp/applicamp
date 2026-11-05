//! Where each good match is credited on the Searches tab (#107): to exactly
//! one source, so "Where the matches came from" adds up to the good matches.
//!
//! The same job can come from several places (two job sites, a job site and
//! the company's own hiring page). The copies merge into one match that
//! keeps every place it came from, and the match is credited to the company
//! page when one is among them (the employer's own listing), else to the
//! site that listed it first.

use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    dedupe,
    job::{Job, SourceLink},
    ports::Scored,
};

/// Where a copy of a job was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Origin {
    /// A company page of the registry.
    Page(Uuid),
    /// A watched company's own job board.
    Watch(Uuid),
    /// A job site, by slug.
    Site(String),
}

impl Origin {
    /// The employer's own listing, not a site's.
    fn is_own(&self) -> bool {
        !matches!(self, Self::Site(_))
    }
}

/// The source `origins` are credited to: the first company page among them,
/// else the first site.
pub(crate) fn credit(origins: &[Origin]) -> Option<&Origin> {
    origins
        .iter()
        .find(|origin| origin.is_own())
        .or_else(|| origins.first())
}

/// Every place `job` was read: the company page or watched board it came
/// from, or the sites that listed it, in the order they did.
pub(crate) fn origins_of(
    job: &Job,
    pages: &HashMap<(String, String), Uuid>,
    watches: &HashMap<(String, String), Uuid>,
) -> Vec<Origin> {
    let key = job.key();
    if let Some(page) = pages.get(&key) {
        return vec![Origin::Page(*page)];
    }
    if let Some(watch) = watches.get(&key) {
        return vec![Origin::Watch(*watch)];
    }
    let Some(board) = &job.board else {
        return Vec::new();
    };
    let mut origins: Vec<Origin> = board
        .links
        .iter()
        .map(|link| Origin::Site(link.board.clone()))
        .collect();
    if origins.is_empty() {
        origins.push(Origin::Site(board.slug.clone()));
    }
    origins
}

/// A good match: one job, however many places it came from.
#[derive(Debug, Clone)]
pub(crate) struct Match {
    /// The copy that is added (the company page's when there is one).
    pub scored: Scored,
    /// The one source it is credited to; none for a job read nowhere the
    /// Searches tab lists (a company board read live).
    pub origin: Option<Origin>,
    /// The links of the other copies, to credit on the opportunity too.
    pub extra_links: Vec<SourceLink>,
}

/// The copies of one job among `kept` (best first) merged into one match
/// each, by apply link, else by company, role and place.
pub(crate) fn merge(kept: Vec<(Scored, Vec<Origin>)>) -> Vec<Match> {
    let mut groups: Vec<Vec<(Scored, Vec<Origin>)>> = Vec::new();
    let mut by_url: HashMap<String, usize> = HashMap::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (scored, origins) in kept {
        let url = dedupe::job_url(&scored.job);
        let key = dedupe::dedupe_key(&scored.job);
        let found = url
            .as_ref()
            .and_then(|u| by_url.get(u))
            .or_else(|| key.as_ref().and_then(|k| by_key.get(k)))
            .copied();
        let at = found.unwrap_or_else(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        if let Some(url) = url {
            by_url.entry(url).or_insert(at);
        }
        if let Some(key) = key {
            by_key.entry(key).or_insert(at);
        }
        groups[at].push((scored, origins));
    }
    groups.into_iter().filter_map(one).collect()
}

fn one(group: Vec<(Scored, Vec<Origin>)>) -> Option<Match> {
    let all: Vec<Origin> = group
        .iter()
        .flat_map(|(_, origins)| origins.iter().cloned())
        .collect();
    let origin = credit(&all).cloned();
    // The copy to add: the first one read on the credited source's kind.
    let at = group
        .iter()
        .position(|(_, origins)| origin.as_ref().is_some_and(|o| origins.contains(o)))
        .unwrap_or(0);
    let known: Vec<String> = group
        .get(at)?
        .0
        .job
        .board
        .iter()
        .flat_map(|b| b.links.iter().map(|l| l.board.clone()))
        .collect();
    let mut extra_links: Vec<SourceLink> = Vec::new();
    for (i, (scored, _)) in group.iter().enumerate() {
        if i == at {
            continue;
        }
        for link in scored.job.board.iter().flat_map(|b| b.links.iter()) {
            let seen =
                known.contains(&link.board) || extra_links.iter().any(|l| l.board == link.board);
            if !seen {
                extra_links.push(link.clone());
            }
        }
    }
    let scored = group.into_iter().nth(at).map(|(s, _)| s)?;
    Some(Match {
        scored,
        origin,
        extra_links,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(slug: &str) -> Origin {
        Origin::Site(slug.to_owned())
    }

    #[test]
    fn a_company_page_wins_over_any_site() {
        let page = Uuid::now_v7();
        let origins = [site("himalayas"), Origin::Page(page), site("remoteok")];
        assert_eq!(credit(&origins), Some(&Origin::Page(page)));
    }

    #[test]
    fn without_a_company_page_the_site_that_listed_it_first_wins() {
        let origins = [site("himalayas"), site("remoteok")];
        assert_eq!(credit(&origins), Some(&site("himalayas")));
        assert_eq!(credit(&[]), None);
    }

    #[test]
    fn a_watched_board_is_the_employers_own_listing_too() {
        let watch = Uuid::now_v7();
        let origins = [site("jobicy"), Origin::Watch(watch)];
        assert_eq!(credit(&origins), Some(&Origin::Watch(watch)));
    }
}
