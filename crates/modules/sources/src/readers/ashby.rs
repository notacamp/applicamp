//! Ashby's public posting API
//! (`/posting-api/job-board/{token}?includeCompensation=true`).

use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, one_line, when};

use super::Board;

/// `/posting-api/job-board/{token}`.
#[derive(Debug, Deserialize)]
pub struct List {
    /// The jobs.
    #[serde(default)]
    pub jobs: Vec<Posting>,
}

/// Pay, when the company publishes it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compensation {
    /// "€80K – €100K • Offers Equity".
    #[serde(default)]
    pub compensation_tier_summary: Option<String>,
}

/// A job.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Posting {
    /// Its id (a UUID).
    pub id: String,
    /// The role.
    #[serde(default)]
    pub title: Option<String>,
    /// Where.
    #[serde(default)]
    pub location: Option<String>,
    /// Remote, as a flag.
    #[serde(default)]
    pub is_remote: Option<bool>,
    /// `Remote`, `Hybrid` or `OnSite`.
    #[serde(default)]
    pub workplace_type: Option<String>,
    /// Listed on the board (unlisted ones are skipped).
    #[serde(default)]
    pub is_listed: Option<bool>,
    /// Its page.
    #[serde(default)]
    pub job_url: Option<String>,
    /// When it was published (RFC 3339).
    #[serde(default)]
    pub published_at: Option<String>,
    /// The description, as text.
    #[serde(default)]
    pub description_plain: Option<String>,
    /// Pay.
    #[serde(default)]
    pub compensation: Option<Compensation>,
}

fn job(posting: Posting, board: &Board) -> Option<Job> {
    if posting.is_listed == Some(false) {
        return None;
    }
    let work_mode = posting
        .workplace_type
        .as_deref()
        .and_then(WorkMode::parse)
        .or_else(|| (posting.is_remote == Some(true)).then_some(WorkMode::Remote));
    // `workplaceType` when it is set, else the `isRemote` flag.
    let board_remote = match posting.workplace_type.as_deref().and_then(WorkMode::parse) {
        Some(mode) => Some(mode == WorkMode::Remote),
        None => posting.is_remote,
    };
    Some(Job {
        board: None,
        source: Source::Ashby,
        source_ref: posting.id.to_ascii_lowercase(),
        title: one_line(posting.title.as_deref())?,
        company: board.company.clone(),
        location: one_line(posting.location.as_deref()),
        work_mode,
        board_remote,
        url: one_line(posting.job_url.as_deref())?,
        posted_at: when(posting.published_at.as_deref()),
        salary_text: one_line(
            posting
                .compensation
                .as_ref()
                .and_then(|c| c.compensation_tier_summary.as_deref()),
        ),
        description: posting
            .description_plain
            .as_deref()
            .unwrap_or("")
            .trim()
            .chars()
            .take(crate::job::MAX_DESCRIPTION)
            .collect(),
    })
}

/// The jobs of `list`, on `board`.
pub fn jobs(list: List, board: &Board) -> Vec<Job> {
    list.jobs
        .into_iter()
        .filter_map(|posting| job(posting, board))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Board {
        Board {
            kind: Source::Ashby,
            token: "exampleco".into(),
            company: "Example Co".into(),
            eu: false,
        }
    }

    #[test]
    fn the_remote_fields_are_read() {
        let list: List = serde_json::from_value(serde_json::json!({ "jobs": [
            { "id": "A", "title": "One", "jobUrl": "https://example.com/1", "isRemote": false },
            { "id": "B", "title": "Two", "jobUrl": "https://example.com/2", "isRemote": true },
            { "id": "C", "title": "Three", "jobUrl": "https://example.com/3",
              "isRemote": true, "workplaceType": "OnSite" },
            { "id": "D", "title": "Four", "jobUrl": "https://example.com/4" },
        ]}))
        .unwrap();
        let jobs = jobs(list, &board());
        let remote: Vec<Option<bool>> = jobs.iter().map(|j| j.board_remote).collect();
        assert_eq!(remote, [Some(false), Some(true), Some(false), None]);
        assert_eq!(
            jobs[0].work_mode, None,
            "not remote says neither hybrid nor on site"
        );
        assert_eq!(jobs[2].work_mode, Some(WorkMode::Onsite));
    }
}
