//! Personio's public XML feed (`https://{token}.jobs.personio.de/xml`): every
//! published position of a company, with its description.
//!
//! Checked on 2026-11-04 with Personio's public demo account: `GET` answers
//! `200` `text/xml` with `<workzag-jobs><position>…`, and `404` for an account
//! that does not exist. `jobs.personio.de/robots.txt` allows everything
//! (`Allow: /`). A small reader for a flat format, like [`super::rss`]: a
//! position's own children by name (descriptions are CDATA).

use chrono::{DateTime, Utc};

use crate::job::{Job, Source, WorkMode, decode_entities, html_to_text, one_line, when};

use super::Board;

/// Positions read per feed, at most.
const MAX_POSITIONS: usize = 500;

/// The text of the first `<name>…</name>` in `inside`, CDATA unwrapped.
fn element(inside: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let from = inside.find(&open)? + open.len();
    let to = inside[from..].find(&close)? + from;
    let raw = inside[from..to].trim();
    Some(
        match raw
            .strip_prefix("<![CDATA[")
            .and_then(|r| r.strip_suffix("]]>"))
        {
            Some(inner) => inner.to_owned(),
            None => decode_entities(raw),
        },
    )
}

/// The `<value>` of every `<jobDescription>` in `inside`, as plain text.
fn description(inside: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = inside;
    while let Some(start) = rest.find("<jobDescription>") {
        let after = &rest[start + "<jobDescription>".len()..];
        let Some(end) = after.find("</jobDescription>") else {
            break;
        };
        let block = &after[..end];
        let heading = element(block, "name").unwrap_or_default();
        let value = element(block, "value")
            .map(|v| html_to_text(&v))
            .unwrap_or_default();
        let part = format!("{}\n{value}", heading.trim()).trim().to_owned();
        if !part.is_empty() {
            parts.push(part);
        }
        rest = &after[end + "</jobDescription>".len()..];
    }
    parts
        .join("\n\n")
        .chars()
        .take(crate::job::MAX_DESCRIPTION)
        .collect()
}

fn salary(inside: &str) -> Option<String> {
    let block = element(inside, "salaryInformation")?;
    let min: f64 = element(&block, "min")?.trim().parse().ok()?;
    let max: f64 = element(&block, "max")?.trim().parse().ok()?;
    let currency = element(&block, "currencyCode").unwrap_or_default();
    let per = element(&block, "type").unwrap_or_default();
    one_line(Some(&format!("{currency} {min:.0}–{max:.0} {per}")))
}

fn posted(raw: Option<&str>) -> Option<DateTime<Utc>> {
    when(raw)
}

/// The jobs of a feed, on `board`.
pub fn jobs(body: &[u8], board: &Board) -> Vec<Job> {
    let text = String::from_utf8_lossy(body);
    let mut jobs = Vec::new();
    let mut rest: &str = &text;
    while jobs.len() < MAX_POSITIONS {
        let Some(start) = rest.find("<position>") else {
            break;
        };
        let after = &rest[start + "<position>".len()..];
        let Some(end) = after.find("</position>") else {
            break;
        };
        let inside = &after[..end];
        rest = &after[end + "</position>".len()..];
        // Its own fields, not the descriptions' headings.
        let own = match (
            inside.find("<jobDescriptions>"),
            inside.find("</jobDescriptions>"),
        ) {
            (Some(a), Some(b)) if a < b => {
                format!(
                    "{}{}",
                    &inside[..a],
                    &inside[b + "</jobDescriptions>".len()..]
                )
            }
            _ => inside.to_owned(),
        };
        let (Some(id), Some(title)) = (
            element(&own, "id").and_then(|v| one_line(Some(&v))),
            element(&own, "name").and_then(|v| one_line(Some(&v))),
        ) else {
            continue;
        };
        let office = element(&own, "office").and_then(|v| one_line(Some(&v)));
        let work_mode = office
            .as_deref()
            .is_some_and(|o| o.eq_ignore_ascii_case("remote"))
            .then_some(WorkMode::Remote);
        jobs.push(Job {
            board: None,
            source: Source::Personio,
            source_ref: id.clone(),
            title,
            company: board.company.clone(),
            location: office,
            work_mode,
            board_remote: work_mode.map(|_| true),
            url: format!("https://{}.jobs.personio.de/job/{id}", board.token),
            posted_at: posted(element(&own, "createdAt").as_deref()),
            salary_text: salary(&own),
            description: description(inside),
        });
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/personio.xml");

    #[test]
    fn the_positions_are_read() {
        let board = Board {
            kind: Source::Personio,
            token: "demo".into(),
            company: "Demo GmbH".into(),
            eu: false,
        };
        let jobs = jobs(FIXTURE.as_bytes(), &board);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].title, "Junior IT-Sicherheitsberater");
        assert_eq!(jobs[0].source_ref, "1822270");
        assert_eq!(jobs[0].url, "https://demo.jobs.personio.de/job/1822270");
        assert_eq!(
            jobs[0].salary_text.as_deref(),
            Some("EUR 45000–55000 yearly")
        );
        assert!(jobs[0].description.contains("Ihre Aufgaben"));
        assert!(jobs[0].description.contains("An invented description."));
        assert_eq!(jobs[0].location.as_deref(), Some("München"));
        assert_eq!(
            jobs[0]
                .posted_at
                .map(|at| at.date_naive().to_string())
                .as_deref(),
            Some("2024-09-03")
        );
        assert_eq!(jobs[1].work_mode, Some(WorkMode::Remote));
        assert_eq!(jobs[1].title, "Initiativbewerbung (Festanstellung)");
    }
}
