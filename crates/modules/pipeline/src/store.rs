//! The pipeline's records, and what other modules may do with them through
//! the ports the server wires: ask which jobs a person already has, and add
//! the ones a search found.

use std::collections::HashSet;

use applicamp_kernel::{EventBus, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::events::OpportunityAdded;

/// Longest title, company, location, next step or pay, in characters.
pub const MAX_SHORT: usize = 300;
/// Longest description, in characters.
pub const MAX_DESCRIPTION: usize = 20_000;
/// Longest URL.
pub const MAX_URL: usize = 2_000;
/// Longest notes, in characters.
pub const MAX_NOTES: usize = 10_000;
/// Reasons kept, and the longest one.
pub const MAX_WHY: (usize, usize) = (3, 200);
/// Gaps kept, and the longest one.
pub const MAX_GAPS: (usize, usize) = (2, 200);

/// The sources a search reads (a link-added one's is `link`).
pub const SEARCH_SOURCES: [&str; 5] = ["greenhouse", "lever", "ashby", "arbeitnow", "hn"];
/// Work modes.
pub const WORK_MODES: [&str; 3] = ["remote", "hybrid", "onsite"];

/// `text` trimmed and cut to `most` characters; `None` when blank.
pub(crate) fn tidy(text: Option<&str>, most: usize) -> Option<String> {
    let text = text?.trim();
    (!text.is_empty()).then(|| text.chars().take(most).collect())
}

fn tidy_list(items: &[String], (most, longest): (usize, usize)) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| tidy(Some(item), longest))
        .take(most)
        .collect()
}

/// A job a search found, scored, to add as Found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// One of [`SEARCH_SOURCES`].
    pub source: String,
    /// Its id at the source.
    pub source_ref: String,
    /// The role.
    pub title: String,
    /// The company.
    pub company: String,
    /// Where.
    pub location: Option<String>,
    /// One of [`WORK_MODES`].
    pub work_mode: Option<String>,
    /// The posting.
    pub url: String,
    /// When it was published.
    pub posted_at: Option<DateTime<Utc>>,
    /// Pay, as written.
    pub salary_text: Option<String>,
    /// Plain text.
    pub description: Option<String>,
    /// 0–100.
    pub match_score: u8,
    /// Why it matches.
    pub why: Vec<String>,
    /// What is missing.
    pub gaps: Vec<String>,
}

/// A new opportunity, checked and trimmed, ready to insert.
#[derive(Debug, Clone, Default)]
pub(crate) struct NewRow {
    pub source: String,
    pub source_ref: String,
    pub title: String,
    pub company: String,
    pub location: Option<String>,
    pub work_mode: Option<String>,
    pub url: String,
    pub posted_at: Option<DateTime<Utc>>,
    pub salary_text: Option<String>,
    pub description: Option<String>,
    pub match_score: Option<i32>,
    pub why: Vec<String>,
    pub gaps: Vec<String>,
}

impl NewRow {
    /// `found` as a row, or `None` when it is not one a search may add (an
    /// unknown source, no reference, title or URL).
    fn of_found(found: Found) -> Option<Self> {
        if !SEARCH_SOURCES.contains(&found.source.as_str()) {
            return None;
        }
        let title = tidy(Some(&found.title), MAX_SHORT)?;
        let url = tidy(Some(&found.url), MAX_URL)?;
        let source_ref = tidy(Some(&found.source_ref), MAX_URL)?;
        Some(Self {
            source: found.source,
            source_ref,
            title,
            company: tidy(Some(&found.company), MAX_SHORT).unwrap_or_default(),
            location: tidy(found.location.as_deref(), MAX_SHORT),
            work_mode: found
                .work_mode
                .filter(|mode| WORK_MODES.contains(&mode.as_str())),
            url,
            posted_at: found.posted_at,
            salary_text: tidy(found.salary_text.as_deref(), MAX_SHORT),
            description: tidy(found.description.as_deref(), MAX_DESCRIPTION),
            match_score: Some(i32::from(found.match_score.min(100))),
            why: tidy_list(&found.why, MAX_WHY),
            gaps: tidy_list(&found.gaps, MAX_GAPS),
        })
    }
}

/// Insert `row` for `account` unless that posting is already in the
/// pipeline, and publish `pipeline.opportunity_added` when it was not. The
/// new opportunity's id, or `None` when it was already there.
pub(crate) async fn insert(
    conn: &mut PgConnection,
    events: &EventBus,
    account: Uuid,
    row: &NewRow,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO pipeline.opportunities
            (id, account_id, title, company, location, work_mode, url, source, source_ref,
             posted_at, salary_text, description, match_score, match_why, match_gaps,
             created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
         ON CONFLICT (account_id, source, source_ref) DO NOTHING
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(&row.title)
    .bind(&row.company)
    .bind(&row.location)
    .bind(&row.work_mode)
    .bind(&row.url)
    .bind(&row.source)
    .bind(&row.source_ref)
    .bind(row.posted_at)
    .bind(&row.salary_text)
    .bind(&row.description)
    .bind(row.match_score)
    .bind(sqlx::types::Json(&row.why))
    .bind(sqlx::types::Json(&row.gaps))
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(opportunity) = id {
        events
            .publish(
                conn,
                &OpportunityAdded {
                    account,
                    opportunity,
                },
            )
            .await?;
    }
    Ok(id)
}

/// The postings in `account`'s pipeline, dismissed ones too, as
/// `(source, source_ref)`: a search skips them.
pub async fn known(db: &PgPool, account: Uuid) -> Result<HashSet<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT source, source_ref FROM pipeline.opportunities WHERE account_id = $1",
    )
    .bind(account)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Add `found` to `account`'s pipeline as Found, in one transaction, each
/// announced with `pipeline.opportunity_added`. Postings already there, and
/// ones that are not valid, are skipped. How many were added.
pub async fn add_found(
    db: &PgPool,
    events: &EventBus,
    account: Uuid,
    found: Vec<Found>,
    now: DateTime<Utc>,
) -> Result<u32> {
    let mut tx = db.begin().await?;
    let mut added = 0_u32;
    for row in found.into_iter().filter_map(NewRow::of_found) {
        if insert(&mut tx, events, account, &row, now).await?.is_some() {
            added += 1;
        }
    }
    tx.commit().await?;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source_ref: &str) -> Found {
        Found {
            source: "greenhouse".into(),
            source_ref: source_ref.into(),
            title: " Staff Engineer ".into(),
            company: "Example Co".into(),
            url: "https://example.com/jobs/1".into(),
            match_score: 120,
            why: vec!["a".into(), " ".into(), "b".into(), "c".into(), "d".into()],
            gaps: vec!["x".repeat(500)],
            work_mode: Some("anywhere".into()),
            ..Found::default()
        }
    }

    #[test]
    fn found_jobs_are_trimmed_and_checked() {
        let row = NewRow::of_found(found("exampleco:1")).unwrap();
        assert_eq!(row.title, "Staff Engineer");
        assert_eq!(row.match_score, Some(100));
        assert_eq!(row.why, ["a", "b", "c"]);
        assert_eq!(row.gaps[0].chars().count(), 200);
        assert_eq!(row.work_mode, None, "an unknown work mode is dropped");
        let unknown = Found {
            source: "linkedin".into(),
            ..found("1")
        };
        assert!(NewRow::of_found(unknown).is_none());
        assert!(
            NewRow::of_found(Found {
                title: " ".into(),
                ..found("2")
            })
            .is_none()
        );
    }

    #[sqlx::test(migrations = "../../../migrations")]
    async fn adding_twice_adds_once(db: PgPool) {
        let account = Uuid::now_v7();
        let events = EventBus::default();
        let now = Utc::now();
        let added = add_found(&db, &events, account, vec![found("a:1"), found("a:2")], now)
            .await
            .unwrap();
        assert_eq!(added, 2);
        let again = add_found(&db, &events, account, vec![found("a:1"), found("a:3")], now)
            .await
            .unwrap();
        assert_eq!(again, 1);
        let have = known(&db, account).await.unwrap();
        assert_eq!(have.len(), 3);
        assert!(have.contains(&("greenhouse".to_owned(), "a:2".to_owned())));
        assert!(known(&db, Uuid::now_v7()).await.unwrap().is_empty());
    }
}
