//! Greenhouse's public job board API
//! (`/v1/boards/{token}/jobs`, `/v1/boards/{token}/jobs/{id}`). The list
//! carries no description; the search reads it, per job, only for the jobs
//! it keeps after its first cut.

use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, decode_entities, html_to_text, one_line, when};

use super::Board;

/// `/v1/boards/{token}/jobs`.
#[derive(Debug, Deserialize)]
pub struct List {
    /// The jobs.
    #[serde(default)]
    pub jobs: Vec<Detail>,
}

/// Where a job is.
#[derive(Debug, Default, Deserialize)]
pub struct Location {
    /// As the company writes it.
    #[serde(default)]
    pub name: Option<String>,
}

/// One job, from the list or on its own (with `content`).
#[derive(Debug, Deserialize)]
pub struct Detail {
    /// Its id.
    pub id: u64,
    /// The role.
    #[serde(default)]
    pub title: Option<String>,
    /// Its page.
    #[serde(default)]
    pub absolute_url: Option<String>,
    /// Where.
    #[serde(default)]
    pub location: Option<Location>,
    /// The company, as Greenhouse names it.
    #[serde(default)]
    pub company_name: Option<String>,
    /// When it was first published (RFC 3339).
    #[serde(default)]
    pub first_published: Option<String>,
    /// When it last changed (RFC 3339).
    #[serde(default)]
    pub updated_at: Option<String>,
    /// The description: HTML, itself HTML-escaped.
    #[serde(default)]
    pub content: Option<String>,
}

/// The description of `detail`, as plain text.
pub fn description(detail: &Detail) -> String {
    detail
        .content
        .as_deref()
        .map(|content| html_to_text(&decode_entities(content)))
        .unwrap_or_default()
}

/// A Greenhouse location says it is remote ("Remote - Europe"): its only
/// structured sign of a work mode.
fn work_mode(location: Option<&str>) -> Option<WorkMode> {
    location
        .is_some_and(|l| l.to_ascii_lowercase().contains("remote"))
        .then_some(WorkMode::Remote)
}

fn job(detail: Detail, board: &Board) -> Option<Job> {
    let title = one_line(detail.title.as_deref())?;
    let url = one_line(detail.absolute_url.as_deref())?;
    let location = one_line(detail.location.as_ref().and_then(|l| l.name.as_deref()));
    Some(Job {
        board: None,
        source: Source::Greenhouse,
        source_ref: format!("{}:{}", board.token, detail.id),
        title,
        company: one_line(detail.company_name.as_deref()).unwrap_or_else(|| board.company.clone()),
        work_mode: work_mode(location.as_deref()),
        board_remote: None,
        location,
        url,
        posted_at: when(detail.first_published.as_deref())
            .or_else(|| when(detail.updated_at.as_deref())),
        salary_text: None,
        description: description(&detail),
    })
}

/// The jobs of `list`, on `board`.
pub fn jobs(list: List, board: &Board) -> Vec<Job> {
    list.jobs
        .into_iter()
        .filter_map(|detail| job(detail, board))
        .collect()
}

/// One job read on its own (a pasted link); the board takes the company's
/// name Greenhouse gives.
pub fn job_of_detail(detail: Detail, board: &mut Board) -> Option<Job> {
    if let Some(company) = one_line(detail.company_name.as_deref()) {
        board.company = company;
    }
    job(detail, board)
}
