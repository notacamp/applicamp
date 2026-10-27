//! The pipeline's records, and what other modules may do with them through
//! the ports the server wires: ask which jobs a person already has, add the
//! ones a search found (with the facts their requirements were checked
//! on), and check the requirements of ones found before the rules (hiding,
//! never deleting, the ones that fail).

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
/// Gaps kept (two from the model, one from the requirement rules), and the
/// longest one.
pub const MAX_GAPS: (usize, usize) = (3, 200);
/// The longest region note.
pub const MAX_REGION_NOTE: usize = 80;
/// Languages kept.
pub const MAX_LANGUAGES: usize = 6;

/// The sources a search reads (a link-added one's is `link`).
pub const SEARCH_SOURCES: [&str; 5] = ["greenhouse", "lever", "ashby", "arbeitnow", "hn"];
/// Work modes.
pub const WORK_MODES: [&str; 3] = ["remote", "hybrid", "onsite"];
/// What a posting says about a fact.
pub const ANSWERS: [&str; 3] = ["yes", "no", "unclear"];
/// Why an opportunity is hidden: it fails a requirement.
pub const HIDDEN_REASONS: [&str; 4] = [
    "not_remote",
    "not_open_to_location",
    "language",
    "below_floor",
];

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

/// What the model read from a posting about the requirements, as the
/// sources module checked them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Eligibility {
    /// It can be done fully remote: one of [`ANSWERS`].
    pub remote: String,
    /// Someone where the person lives can do it without moving: one of
    /// [`ANSWERS`].
    pub open_to_candidate: String,
    /// Where it is open, as the posting says.
    pub region_note: Option<String>,
    /// The languages it needs, ISO 639-1.
    pub working_languages: Vec<String>,
}

impl Eligibility {
    /// Checked and trimmed; `None` when an answer is not one of [`ANSWERS`].
    pub(crate) fn tidied(self) -> Option<Self> {
        if !ANSWERS.contains(&self.remote.as_str())
            || !ANSWERS.contains(&self.open_to_candidate.as_str())
        {
            return None;
        }
        let mut languages: Vec<String> = Vec::new();
        for code in &self.working_languages {
            let code = code.trim().to_ascii_lowercase();
            let valid =
                (2..=3).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase());
            if valid && !languages.contains(&code) && languages.len() < MAX_LANGUAGES {
                languages.push(code);
            }
        }
        Some(Self {
            region_note: tidy(self.region_note.as_deref(), MAX_REGION_NOTE),
            working_languages: languages,
            ..self
        })
    }
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
    /// The facts its requirements were checked on.
    pub eligibility: Option<Eligibility>,
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
    pub eligibility: Option<Eligibility>,
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
            eligibility: found.eligibility.and_then(Eligibility::tidied),
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
    let eligibility = row.eligibility.as_ref();
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO pipeline.opportunities
            (id, account_id, title, company, location, work_mode, url, source, source_ref,
             posted_at, salary_text, description, match_score, match_why, match_gaps,
             created_at, updated_at, remote, open_to_candidate, region_note, working_languages)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16,
                 $17, $18, $19, $20)
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
    .bind(eligibility.map(|e| e.remote.as_str()))
    .bind(eligibility.map(|e| e.open_to_candidate.as_str()))
    .bind(eligibility.and_then(|e| e.region_note.as_deref()))
    .bind(eligibility.map(|e| e.working_languages.clone()))
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

/// An opportunity a search found before the requirement rules.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UncheckedRow {
    pub id: Uuid,
    pub account_id: Uuid,
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
}

/// Up to `most` Found or Shortlisted opportunities a search found (they
/// have a match) whose requirements were never checked, still in view;
/// newest first. Opportunities added by link are the person's own choice
/// and are never checked.
pub async fn unchecked(db: &PgPool, most: usize) -> Result<Vec<UncheckedRow>> {
    Ok(sqlx::query_as(
        "SELECT id, account_id, source, source_ref, title, company, location, work_mode, url,
                posted_at, salary_text, description
           FROM pipeline.opportunities
          WHERE stage IN ('found', 'shortlisted') AND remote IS NULL AND hidden_reason IS NULL
            AND match_score IS NOT NULL AND source <> 'link' AND NOT dismissed
          ORDER BY created_at DESC, id
          LIMIT $1",
    )
    .bind(i64::try_from(most).unwrap_or(i64::MAX))
    .fetch_all(db)
    .await?)
}

/// Keep `eligibility` on `account`'s opportunity `id`, and hide it for
/// `hidden_reason` (one of [`HIDDEN_REASONS`]) when it fails a requirement:
/// only while it is still Found or Shortlisted and unchecked. Nothing else
/// changes, and nothing is deleted. Whether it was kept.
pub async fn set_eligibility(
    db: &PgPool,
    account: Uuid,
    id: Uuid,
    eligibility: Eligibility,
    hidden_reason: Option<&str>,
) -> Result<bool> {
    let Some(eligibility) = eligibility.tidied() else {
        return Ok(false);
    };
    let hidden_reason = hidden_reason.filter(|reason| HIDDEN_REASONS.contains(reason));
    let done = sqlx::query(
        "UPDATE pipeline.opportunities
            SET remote = $3, open_to_candidate = $4, region_note = $5, working_languages = $6,
                hidden_reason = $7
          WHERE id = $1 AND account_id = $2 AND stage IN ('found', 'shortlisted')
            AND remote IS NULL",
    )
    .bind(id)
    .bind(account)
    .bind(&eligibility.remote)
    .bind(&eligibility.open_to_candidate)
    .bind(&eligibility.region_note)
    .bind(&eligibility.working_languages)
    .bind(hidden_reason)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
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
    fn eligibility_is_checked_and_trimmed() {
        let eligibility = Eligibility {
            remote: "yes".into(),
            open_to_candidate: "no".into(),
            region_note: Some(format!(" {} ", "x".repeat(100))),
            working_languages: vec!["EN".into(), "en".into(), "German".into(), "de".into()],
        }
        .tidied()
        .unwrap();
        assert_eq!(eligibility.region_note.map(|n| n.chars().count()), Some(80));
        assert_eq!(eligibility.working_languages, ["en", "de"]);
        let unknown = Eligibility {
            remote: "maybe".into(),
            ..Eligibility::default()
        };
        assert!(unknown.tidied().is_none());
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

    fn remote_only_from_germany() -> Eligibility {
        Eligibility {
            remote: "yes".into(),
            open_to_candidate: "no".into(),
            region_note: Some("Germany or Austria only".into()),
            working_languages: vec!["en".into()],
        }
    }

    #[sqlx::test(migrations = "../../../migrations")]
    async fn matches_from_before_the_rules_are_checked_once_and_hidden_never_deleted(db: PgPool) {
        let account = Uuid::now_v7();
        let events = EventBus::default();
        let now = Utc::now();
        let checked = Found {
            eligibility: Some(Eligibility {
                remote: "yes".into(),
                open_to_candidate: "yes".into(),
                ..Eligibility::default()
            }),
            ..found("a:4")
        };
        let added = add_found(
            &db,
            &events,
            account,
            vec![found("a:1"), found("a:2"), found("a:3"), checked],
            now,
        )
        .await
        .unwrap();
        assert_eq!(added, 4);
        // One moved on to Applying: the person's choice, never touched.
        sqlx::query(
            "UPDATE pipeline.opportunities SET stage = 'applying'
              WHERE account_id = $1 AND source_ref = 'a:3'",
        )
        .bind(account)
        .execute(&db)
        .await
        .unwrap();
        // One added by link: never checked.
        sqlx::query(
            "INSERT INTO pipeline.opportunities
                (id, account_id, title, company, url, source, source_ref, created_at, updated_at)
             VALUES ($1, $2, 'Mine', '', 'https://example.org/1', 'link',
                     'https://example.org/1', $3, $3)",
        )
        .bind(Uuid::now_v7())
        .bind(account)
        .bind(now)
        .execute(&db)
        .await
        .unwrap();

        let rows = unchecked(&db, 30).await.unwrap();
        let refs: Vec<&str> = rows.iter().map(|r| r.source_ref.as_str()).collect();
        assert_eq!(refs.len(), 2, "{refs:?}");
        assert!(refs.contains(&"a:1") && refs.contains(&"a:2"));
        let first = rows.iter().find(|r| r.source_ref == "a:1").unwrap();
        assert!(
            set_eligibility(
                &db,
                account,
                first.id,
                remote_only_from_germany(),
                Some("not_open_to_location")
            )
            .await
            .unwrap()
        );
        // Checked once: never again.
        assert!(
            !set_eligibility(&db, account, first.id, remote_only_from_germany(), None)
                .await
                .unwrap()
        );
        // Not someone else's.
        let second = rows.iter().find(|r| r.source_ref == "a:2").unwrap();
        assert!(
            !set_eligibility(
                &db,
                Uuid::now_v7(),
                second.id,
                remote_only_from_germany(),
                None
            )
            .await
            .unwrap()
        );
        let left: Vec<String> = unchecked(&db, 30)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.source_ref)
            .collect();
        assert_eq!(left, ["a:2"]);
        let (count, hidden): (i64, Option<String>) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM pipeline.opportunities WHERE account_id = $1),
                    (SELECT hidden_reason FROM pipeline.opportunities WHERE id = $2)",
        )
        .bind(account)
        .bind(first.id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(count, 5, "nothing deleted");
        assert_eq!(hidden.as_deref(), Some("not_open_to_location"));
    }
}
