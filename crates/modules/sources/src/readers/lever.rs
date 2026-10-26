//! Lever's public postings API (`/v0/postings/{token}?mode=json`, and
//! `/v0/postings/{token}/{id}`), on `api.lever.co` or the EU instance.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, html_to_text, one_line};

use super::Board;

/// A posting's categories.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Categories {
    /// Where.
    #[serde(default)]
    pub location: Option<String>,
    /// Every place it is open in.
    #[serde(default)]
    pub all_locations: Vec<String>,
    /// Full-time, contract…
    #[serde(default)]
    pub commitment: Option<String>,
}

/// One section of a posting ("What you'll do").
#[derive(Debug, Default, Deserialize)]
pub struct Section {
    /// Its heading.
    #[serde(default)]
    pub text: Option<String>,
    /// Its content, HTML.
    #[serde(default)]
    pub content: Option<String>,
}

/// Pay, when the company publishes it.
#[derive(Debug, Default, Deserialize)]
pub struct SalaryRange {
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub interval: Option<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
}

/// A posting.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Posting {
    /// Its id (a UUID).
    pub id: String,
    /// The role.
    #[serde(default)]
    pub text: Option<String>,
    /// Its page.
    #[serde(default)]
    pub hosted_url: Option<String>,
    /// When it was created, in milliseconds since 1970.
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Where, and how.
    #[serde(default)]
    pub categories: Categories,
    /// `remote`, `hybrid`, `on-site` or `unspecified`.
    #[serde(default)]
    pub workplace_type: Option<String>,
    /// The description, as text.
    #[serde(default)]
    pub description_plain: Option<String>,
    /// Its sections.
    #[serde(default)]
    pub lists: Vec<Section>,
    /// The closing part, as text.
    #[serde(default)]
    pub additional_plain: Option<String>,
    /// Pay.
    #[serde(default)]
    pub salary_range: Option<SalaryRange>,
}

fn salary(range: Option<&SalaryRange>) -> Option<String> {
    let range = range?;
    let (min, max) = (range.min?, range.max?);
    let currency = range.currency.as_deref().unwrap_or("");
    let interval = range.interval.as_deref().unwrap_or("");
    one_line(Some(&format!("{currency} {min:.0}–{max:.0} {interval}")))
}

fn description(posting: &Posting) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(text) = posting.description_plain.as_deref() {
        parts.push(text.trim().to_owned());
    }
    for section in &posting.lists {
        let heading = section.text.as_deref().unwrap_or("").trim();
        let content = section
            .content
            .as_deref()
            .map(html_to_text)
            .unwrap_or_default();
        parts.push(format!("{heading}\n{content}").trim().to_owned());
    }
    if let Some(text) = posting.additional_plain.as_deref() {
        parts.push(text.trim().to_owned());
    }
    let text = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    text.chars().take(crate::job::MAX_DESCRIPTION).collect()
}

/// One posting on `board`.
pub fn job(posting: Posting, board: &Board) -> Option<Job> {
    let title = one_line(posting.text.as_deref())?;
    let url = one_line(posting.hosted_url.as_deref())?;
    let location = one_line(posting.categories.location.as_deref())
        .or_else(|| one_line(Some(&posting.categories.all_locations.join(", "))));
    Some(Job {
        source: Source::Lever,
        description: description(&posting),
        salary_text: salary(posting.salary_range.as_ref()),
        source_ref: posting.id.to_ascii_lowercase(),
        title,
        company: board.company.clone(),
        location,
        work_mode: posting.workplace_type.as_deref().and_then(WorkMode::parse),
        url,
        posted_at: posting
            .created_at
            .and_then(DateTime::<Utc>::from_timestamp_millis),
    })
}

/// The postings of `list`, on `board`.
pub fn jobs(list: Vec<Posting>, board: &Board) -> Vec<Job> {
    list.into_iter()
        .filter_map(|posting| job(posting, board))
        .collect()
}
