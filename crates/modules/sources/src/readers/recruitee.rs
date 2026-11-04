//! Recruitee's public offers API (`https://{token}.recruitee.com/api/offers/`):
//! the published offers of a company, with their description.
//!
//! Checked on 2026-11-04 with Recruitee's own public demo account: `GET`
//! answers `200` with `{ offers: [...] }`, and `404` for a company that does
//! not exist. A company on its own domain redirects its robots.txt there
//! (the reader never follows redirects: such a company is skipped). The
//! robots.txt of a company's `recruitee.com` address disallows only
//! `/v/` (`personio.recruitee.com`), so the offers path is allowed.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use crate::job::{Job, Source, WorkMode, html_to_text, one_line};

use super::Board;

/// `/api/offers/`.
#[derive(Debug, Deserialize)]
pub struct List {
    #[serde(default)]
    pub offers: Vec<Offer>,
}

/// Pay, when published.
#[derive(Debug, Default, Deserialize)]
pub struct Salary {
    #[serde(default)]
    pub min: Option<serde_json::Value>,
    #[serde(default)]
    pub max: Option<serde_json::Value>,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

/// One offer.
#[derive(Debug, Deserialize)]
pub struct Offer {
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub company_name: Option<String>,
    /// Its page.
    #[serde(default)]
    pub careers_url: Option<String>,
    /// Where to apply.
    #[serde(default)]
    pub careers_apply_url: Option<String>,
    /// `2026-10-20 14:22:42 UTC`.
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub remote: bool,
    #[serde(default)]
    pub hybrid: bool,
    #[serde(default)]
    pub on_site: bool,
    /// HTML.
    #[serde(default)]
    pub description: Option<String>,
    /// HTML.
    #[serde(default)]
    pub requirements: Option<String>,
    #[serde(default)]
    pub salary: Option<Salary>,
    /// `published` for an open offer.
    #[serde(default)]
    pub status: Option<String>,
}

fn when(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let text = raw?.trim().trim_end_matches(" UTC");
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|at| at.and_utc())
}

fn amount(value: Option<&serde_json::Value>) -> Option<f64> {
    match value? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn salary(salary: Option<&Salary>) -> Option<String> {
    let salary = salary?;
    let (min, max) = (amount(salary.min.as_ref())?, amount(salary.max.as_ref())?);
    one_line(Some(&format!(
        "{} {min:.0}–{max:.0} {}",
        salary.currency.as_deref().unwrap_or(""),
        salary.period.as_deref().unwrap_or("")
    )))
}

fn job(offer: &Offer, board: &Board) -> Option<Job> {
    if offer.status.as_deref().is_some_and(|s| s != "published") {
        return None;
    }
    let title = one_line(offer.title.as_deref())?;
    let url = one_line(offer.careers_url.as_deref())?;
    let work_mode = if offer.remote {
        Some(WorkMode::Remote)
    } else if offer.hybrid {
        Some(WorkMode::Hybrid)
    } else if offer.on_site {
        Some(WorkMode::Onsite)
    } else {
        None
    };
    let mut description = String::new();
    for html in [offer.description.as_deref(), offer.requirements.as_deref()]
        .into_iter()
        .flatten()
    {
        let text = html_to_text(html);
        if !text.is_empty() {
            if !description.is_empty() {
                description.push_str("\n\n");
            }
            description.push_str(&text);
        }
    }
    Some(Job {
        board: None,
        source: Source::Recruitee,
        source_ref: offer.id.to_string(),
        title,
        company: one_line(offer.company_name.as_deref()).unwrap_or_else(|| board.company.clone()),
        location: one_line(offer.location.as_deref()),
        work_mode,
        board_remote: work_mode.map(|mode| mode == WorkMode::Remote),
        url: one_line(offer.careers_apply_url.as_deref()).unwrap_or(url),
        posted_at: when(offer.published_at.as_deref()),
        salary_text: salary(offer.salary.as_ref()),
        description: description
            .chars()
            .take(crate::job::MAX_DESCRIPTION)
            .collect(),
    })
}

/// The jobs of `list`, on `board`.
pub fn jobs(list: &List, board: &Board) -> Vec<Job> {
    list.offers.iter().filter_map(|o| job(o, board)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/recruitee.json");

    #[test]
    fn the_offers_are_read() {
        let list: List = serde_json::from_str(FIXTURE).unwrap();
        let board = Board {
            kind: Source::Recruitee,
            token: "personio".into(),
            company: "Fallback".into(),
            eu: false,
        };
        let jobs = jobs(&list, &board);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].company, "FD Sandbox");
        assert_eq!(jobs[0].source_ref, "1968676");
        assert_eq!(jobs[0].work_mode, Some(WorkMode::Onsite));
        assert!(jobs[0].description.contains("An invented job"));
        assert_eq!(
            jobs[0]
                .posted_at
                .map(|at| at.date_naive().to_string())
                .as_deref(),
            Some("2026-10-20")
        );
        assert_eq!(jobs[1].work_mode, Some(WorkMode::Remote));
        assert_eq!(jobs[1].salary_text.as_deref(), Some("EUR 42000–52000 year"));
    }
}
