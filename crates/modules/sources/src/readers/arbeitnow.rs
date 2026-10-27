//! Arbeitnow's job board API (`/api/job-board-api?page=N`): jobs in Europe,
//! newest first, a hundred a page.

use chrono::DateTime;
use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, html_to_text, one_line};

/// A page.
#[derive(Debug, Deserialize)]
pub struct Page {
    /// Its jobs.
    #[serde(default)]
    pub data: Vec<Posting>,
    /// The other pages.
    #[serde(default)]
    pub links: Links,
}

/// Links to the other pages.
#[derive(Debug, Default, Deserialize)]
pub struct Links {
    /// The next page, if any.
    #[serde(default)]
    pub next: Option<String>,
}

/// A job.
#[derive(Debug, Deserialize)]
pub struct Posting {
    /// Its id.
    pub slug: String,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// HTML.
    #[serde(default)]
    pub description: Option<String>,
    /// Remote, as a flag (false says nothing).
    #[serde(default)]
    pub remote: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// Seconds since 1970.
    #[serde(default)]
    pub created_at: Option<i64>,
}

/// The jobs of `page`.
pub fn jobs(page: Page) -> Vec<Job> {
    page.data
        .into_iter()
        .filter_map(|posting| {
            Some(Job {
                source: Source::Arbeitnow,
                title: one_line(posting.title.as_deref())?,
                company: one_line(posting.company_name.as_deref())?,
                url: one_line(posting.url.as_deref())?,
                location: one_line(posting.location.as_deref()),
                work_mode: (posting.remote == Some(true)).then_some(WorkMode::Remote),
                // Its `remote` flag is set for jobs remote only within a
                // country: never a requirement's evidence.
                board_remote: None,
                posted_at: posting
                    .created_at
                    .and_then(|secs| DateTime::from_timestamp(secs, 0)),
                salary_text: None,
                description: posting
                    .description
                    .as_deref()
                    .map(html_to_text)
                    .unwrap_or_default(),
                source_ref: posting.slug,
            })
        })
        .collect()
}
