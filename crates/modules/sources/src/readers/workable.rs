//! Workable's public widget API (`https://apply.workable.com/api/v1/widget/accounts/{token}`):
//! the open jobs of a company's account, with no description in the list.
//!
//! Checked on 2026-11-04 with a public company: `GET` answers `200` with
//! `{ name, description, jobs: [...] }`, and `404` for an account that does
//! not exist. `apply.workable.com/robots.txt` allows everything for every
//! agent (`Disallow:` empty) and carries
//! `Content-Signal: search=yes, ai-input=yes, ai-train=no`, which the hiring
//! system's own check keeps and sends to the admin as a choice.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, one_line};

use super::Board;

/// `/api/v1/widget/accounts/{token}`.
#[derive(Debug, Deserialize)]
pub struct List {
    /// The company's name.
    #[serde(default)]
    pub name: Option<String>,
    /// The open jobs.
    #[serde(default)]
    pub jobs: Vec<Posting>,
}

/// A job's place.
#[derive(Debug, Default, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

/// One open job.
#[derive(Debug, Deserialize)]
pub struct Posting {
    /// Its short code, its id.
    pub shortcode: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Its page.
    #[serde(default)]
    pub url: Option<String>,
    /// Where to apply.
    #[serde(default)]
    pub application_url: Option<String>,
    /// The day it was published (`2026-10-20`).
    #[serde(default)]
    pub published_on: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Whether it can be done from anywhere.
    #[serde(default)]
    pub telecommuting: bool,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub locations: Vec<Location>,
}

fn day(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(raw?.trim(), "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc())
}

fn place(posting: &Posting) -> Option<String> {
    let visible: Vec<String> = posting
        .locations
        .iter()
        .filter(|l| !l.hidden)
        .filter_map(|l| {
            let parts: Vec<&str> = [l.city.as_deref(), l.country.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        })
        .collect();
    if let Some(first) = visible.first() {
        return Some(first.clone());
    }
    let parts: Vec<&str> = [posting.city.as_deref(), posting.country.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn job(posting: &Posting, company: &str) -> Option<Job> {
    let title = one_line(posting.title.as_deref())?;
    let url = one_line(posting.url.as_deref())?;
    let work_mode = posting.telecommuting.then_some(WorkMode::Remote);
    Some(Job {
        board: None,
        source: Source::Workable,
        source_ref: posting.shortcode.to_ascii_lowercase(),
        title,
        company: company.to_owned(),
        location: place(posting),
        work_mode,
        board_remote: work_mode.map(|_| true),
        url: one_line(posting.application_url.as_deref()).unwrap_or(url),
        posted_at: day(posting.published_on.as_deref())
            .or_else(|| day(posting.created_at.as_deref())),
        salary_text: None,
        description: String::new(),
    })
}

/// The jobs of `list`, on `board`.
pub fn jobs(list: &List, board: &Board) -> Vec<Job> {
    let company = one_line(list.name.as_deref()).unwrap_or_else(|| board.company.clone());
    list.jobs
        .iter()
        .filter_map(|posting| job(posting, &company))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/workable.json");

    #[test]
    fn the_widget_list_is_read() {
        let list: List = serde_json::from_str(FIXTURE).unwrap();
        let board = Board {
            kind: Source::Workable,
            token: "huggingface".into(),
            company: "Fallback".into(),
            eu: false,
        };
        let jobs = jobs(&list, &board);
        assert_eq!(jobs.len(), 3);
        let first = &jobs[0];
        assert_eq!(first.company, "Hugging Face");
        assert_eq!(first.source, Source::Workable);
        assert_eq!(first.source_ref, "81b46579fe");
        assert_eq!(first.work_mode, Some(WorkMode::Remote));
        assert_eq!(first.location.as_deref(), Some("Paris, France"));
        assert!(first.url.ends_with("/j/81B46579FE/apply"));
        assert_eq!(
            first
                .posted_at
                .map(|at| at.date_naive().to_string())
                .as_deref(),
            Some("2026-05-29")
        );
    }
}
