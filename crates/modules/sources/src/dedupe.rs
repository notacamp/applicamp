//! The same job on several boards is one opportunity.
//!
//! Two jobs are the same when their apply links are the same once tracking
//! parameters are stripped ([`canonical_url`]), or else when their company,
//! role and place are ([`dedupe_key`], within 30 days for jobs already in
//! the pipeline). Within one search the copies merge into the first, which
//! lists every board it was seen on ([`merge`]); a job already in the
//! pipeline under another board gets that board added to its sources.

use std::collections::HashMap;

use url::Url;

use crate::{job::Job, places};

/// Query parameters that only track where a click came from.
const TRACKING: [&str; 14] = [
    "ref",
    "source",
    "src",
    "gh_src",
    "referrer",
    "referer",
    "lever-source",
    "lever-origin",
    "trk",
    "fbclid",
    "gclid",
    "mc_cid",
    "mc_eid",
    "via",
];

/// `raw` as a link compares: https, the host in lower case without
/// `www.`, the path without a trailing `/`, the query without tracking
/// parameters (`utm_*`, `ref`, `source`, `gh_src`, …) and sorted, no
/// fragment. `None` when it is not an http(s) link.
pub fn canonical_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let path = url.path().trim_end_matches('/');
    let mut query: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !key.starts_with("utm_") && !TRACKING.contains(&key.as_str())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    query.sort();
    let mut canonical = format!("https://{host}{path}");
    if !query.is_empty() {
        let pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        canonical.push('?');
        canonical.push_str(&pairs.join("&"));
    }
    Some(canonical.chars().take(2_000).collect())
}

/// The link `job` is compared by: its apply link, else its posting's.
pub fn job_url(job: &Job) -> Option<String> {
    job.board
        .as_ref()
        .and_then(|b| b.apply_url.as_deref())
        .and_then(canonical_url)
        .or_else(|| canonical_url(&job.url))
}

fn words(text: &str) -> String {
    places::words(text).join(" ")
}

/// `company|role|place` in lower case, the place as the countries its
/// location and regions name (sorted; nothing when they name none, as for
/// a job open anywhere). `None` without a company or a role (a free-text
/// post not read yet).
pub fn dedupe_key(job: &Job) -> Option<String> {
    let company = words(&job.company);
    let title = words(&job.title);
    if company.is_empty() || title.is_empty() {
        return None;
    }
    let mut countries: Vec<&str> = job
        .location
        .as_deref()
        .map(places::countries_in)
        .unwrap_or_default();
    let board_regions = job
        .board
        .as_ref()
        .map(|b| b.regions.as_slice())
        .unwrap_or(&[]);
    for region in board_regions {
        for code in places::countries_in(region) {
            if !countries.contains(&code) {
                countries.push(code);
            }
        }
    }
    countries.sort_unstable();
    let place = countries.join(",");
    Some(
        format!("{company}|{title}|{place}")
            .chars()
            .take(700)
            .collect(),
    )
}

/// `jobs` with the copies of a job seen on several boards merged into the
/// first, which lists every board's link. Jobs from company boards (no job
/// board) are never merged.
pub fn merge(jobs: Vec<Job>) -> Vec<Job> {
    let mut out: Vec<Job> = Vec::with_capacity(jobs.len());
    let mut by_url: HashMap<String, usize> = HashMap::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for job in jobs {
        if job.board.is_none() {
            out.push(job);
            continue;
        }
        let url = job_url(&job);
        let key = dedupe_key(&job);
        let found = url
            .as_ref()
            .and_then(|u| by_url.get(u))
            .or_else(|| key.as_ref().and_then(|k| by_key.get(k)))
            .copied();
        if let Some(index) = found {
            let links = job.board.map(|b| b.links).unwrap_or_default();
            if let Some(first) = out.get_mut(index).and_then(|j| j.board.as_mut()) {
                for link in links {
                    if !first.links.iter().any(|l| l.board == link.board) {
                        first.links.push(link);
                    }
                }
            }
            continue;
        }
        let index = out.len();
        if let Some(url) = url {
            by_url.insert(url, index);
        }
        if let Some(key) = key {
            by_key.insert(key, index);
        }
        out.push(job);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{BoardJob, Source, SourceLink};

    fn board_job(board: &str, apply: &str, company: &str, title: &str) -> Job {
        Job {
            source: Source::Board,
            source_ref: format!("{board}-1"),
            title: title.into(),
            company: company.into(),
            location: None,
            work_mode: None,
            board_remote: None,
            url: format!("https://{board}.example/jobs/1"),
            posted_at: None,
            salary_text: None,
            description: String::new(),
            board: Some(BoardJob {
                slug: board.into(),
                links: vec![SourceLink {
                    board: board.into(),
                    name: board.to_uppercase(),
                    url: format!("https://{board}.example/jobs/1"),
                }],
                apply_url: Some(apply.into()),
                regions: vec!["Anywhere".into()],
                ..BoardJob::default()
            }),
        }
    }

    #[test]
    fn links_compare_without_tracking() {
        assert_eq!(
            canonical_url(
                "https://WWW.Example.com/jobs/1/?utm_source=api&ref=x&id=7&gh_src=y#apply"
            )
            .as_deref(),
            Some("https://example.com/jobs/1?id=7")
        );
        assert_eq!(
            canonical_url("http://example.com/jobs/1"),
            canonical_url("https://example.com/jobs/1?utm_medium=feed")
        );
        assert_eq!(canonical_url("mailto:jobs@example.com"), None);
    }

    #[test]
    fn the_same_job_on_two_boards_is_one_with_both_sources() {
        let jobs = vec![
            board_job(
                "himalayas",
                "https://jobs.example.com/apply/42?utm_source=api",
                "Example Co",
                "Staff Engineer",
            ),
            board_job(
                "remoteok",
                "https://jobs.example.com/apply/42",
                "Example Co",
                "Staff Engineer",
            ),
            board_job(
                "jobicy",
                "https://jobicy.example/jobs/9",
                "Example Co.",
                "Staff  engineer",
            ),
            board_job(
                "jobicy",
                "https://jobicy.example/jobs/10",
                "Other Co",
                "Staff Engineer",
            ),
        ];
        let merged = merge(jobs);
        assert_eq!(merged.len(), 2);
        let boards: Vec<&str> = merged[0]
            .board
            .as_ref()
            .unwrap()
            .links
            .iter()
            .map(|l| l.board.as_str())
            .collect();
        assert_eq!(boards, ["himalayas", "remoteok", "jobicy"]);
        assert_eq!(merged[1].company, "Other Co");
        assert_eq!(
            dedupe_key(&merged[0]).as_deref(),
            Some("example co|staff engineer|")
        );
    }
}
