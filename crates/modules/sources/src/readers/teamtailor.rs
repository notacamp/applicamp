//! Teamtailor's public job feed (`https://{token}.teamtailor.com/jobs.rss`):
//! the open jobs of a company, with their description.
//!
//! Checked on 2026-11-04 with Teamtailor's own careers site: `GET` answers
//! `200` `application/rss+xml`, and `404` for a company that does not exist.
//! `career.teamtailor.com/robots.txt` disallows `/app/`, `/messages/`,
//! `/messenger/`, `/facebook/tab/` and `/jobs/internal/` and carries
//! `Content-Signal: search=yes, ai-train=no, ai-input=yes`: the feed path is
//! allowed, and the signal is the hiring system's choice for the admin.
//! A flat feed, read by [`super::rss`]; a job's place is nested in
//! `tt:locations`, read from there.

use std::sync::LazyLock;

use regex::Regex;

use crate::job::{Job, Source, WorkMode, html_to_text, one_line};

use super::{Board, rss};

static CITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<tt:city>(.*?)</tt:city>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static COUNTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<tt:country>(.*?)</tt:country>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

fn capture(regex: &Regex, text: &str) -> Option<String> {
    let found = regex.captures(text)?.get(1)?.as_str();
    one_line(Some(found))
}

fn place(locations: &str) -> Option<String> {
    let parts: Vec<String> = [capture(&CITY, locations), capture(&COUNTRY, locations)]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn posted(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc2822(raw.trim())
        .ok()
        .map(|at| at.with_timezone(&chrono::Utc))
}

/// The jobs of a feed, on `board`.
pub fn jobs(body: &[u8], board: &Board) -> Vec<Job> {
    rss::items(body)
        .iter()
        .filter_map(|item| {
            let field = |name: &str| item.0.get(name).map(String::as_str);
            let title = one_line(field("title"))?;
            let url = one_line(field("link"))?;
            let work_mode = match field("remoteStatus").map(str::trim) {
                Some("fully") => Some(WorkMode::Remote),
                Some("hybrid") => Some(WorkMode::Hybrid),
                Some("none") => Some(WorkMode::Onsite),
                _ => None,
            };
            Some(Job {
                board: None,
                source: Source::Teamtailor,
                source_ref: one_line(field("guid")).unwrap_or_else(|| url.clone()),
                title,
                company: board.company.clone(),
                location: field("tt:locations").and_then(place),
                work_mode,
                board_remote: work_mode.map(|mode| mode == WorkMode::Remote),
                url,
                posted_at: field("pubDate").and_then(posted),
                salary_text: None,
                description: field("description").map(html_to_text).unwrap_or_default(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/teamtailor.rss");

    #[test]
    fn the_feed_is_read() {
        let board = Board {
            kind: Source::Teamtailor,
            token: "career".into(),
            company: "Teamtailor".into(),
            eu: false,
        };
        let jobs = jobs(FIXTURE.as_bytes(), &board);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].title, "Customer Success Manager - Finnish speaking");
        assert_eq!(jobs[0].location.as_deref(), Some("Stockholm, Sweden"));
        assert_eq!(jobs[0].work_mode, Some(WorkMode::Onsite));
        assert_eq!(jobs[0].description, "An invented description of the job.");
        assert!(
            jobs[0]
                .url
                .starts_with("https://career.teamtailor.com/jobs/")
        );
        assert!(jobs[0].posted_at.is_some());
    }
}
