//! The pipeline's records, and what other modules may do with them through
//! the ports the server wires: ask which jobs a person already has (by
//! source, by link, by company, role and place), add the ones a search
//! found (with the facts their requirements were checked on, and every job
//! board they were seen on), credit another board on one already there,
//! and check the requirements of ones found before the rules (hiding, never
//! deleting, the ones that fail).

use std::collections::HashSet;

use applicamp_kernel::{EventBus, Result};
use chrono::TimeDelta;
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

/// The source of a link-added one.
pub const LINK: &str = "link";
/// Job boards credited per opportunity, at most.
pub const MAX_SOURCES: usize = 20;

/// Whether `source` is one a search may add: a job board's slug or a
/// reader's name (lower-case letters, digits and `-`, at most 40), never
/// `link`.
pub fn is_search_source(source: &str) -> bool {
    let mut chars = source.chars();
    source != LINK
        && source.len() <= 40
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A job board an opportunity was seen on, with the posting's link there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceLink {
    /// The board's slug.
    pub board: String,
    /// How it is credited ("Himalayas").
    pub name: String,
    /// The posting on it.
    pub url: String,
}

impl SourceLink {
    /// Checked and trimmed; `None` when it is not a board and a web link.
    fn tidied(&self) -> Option<Self> {
        let url = tidy(Some(&self.url), MAX_URL)?;
        let web = url::Url::parse(&url)
            .ok()
            .is_some_and(|u| matches!(u.scheme(), "http" | "https"));
        (is_search_source(&self.board) && web).then(|| Self {
            board: self.board.clone(),
            name: tidy(Some(&self.name), 100).unwrap_or_else(|| self.board.clone()),
            url,
        })
    }
}
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
    /// The board's apply link, unchanged.
    pub apply_url: Option<String>,
    /// When it expires (a board whose terms drop expired jobs).
    pub expires_at: Option<DateTime<Utc>>,
    /// Its apply link without tracking, to dedupe across boards.
    pub canonical_url: Option<String>,
    /// Its company, role and place, to dedupe across boards.
    pub dedupe_key: Option<String>,
    /// Every job board it was seen on.
    pub sources: Vec<SourceLink>,
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
    pub apply_url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub canonical_url: Option<String>,
    pub dedupe_key: Option<String>,
    pub sources: Vec<SourceLink>,
}

impl NewRow {
    /// `found` as a row, or `None` when it is not one a search may add (an
    /// unknown source, no reference, title or URL).
    fn of_found(found: Found) -> Option<Self> {
        if !is_search_source(&found.source) {
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
            apply_url: tidy(found.apply_url.as_deref(), MAX_URL),
            expires_at: found.expires_at,
            canonical_url: tidy(found.canonical_url.as_deref(), MAX_URL),
            dedupe_key: tidy(found.dedupe_key.as_deref(), 700),
            sources: found
                .sources
                .iter()
                .filter_map(SourceLink::tidied)
                .take(MAX_SOURCES)
                .collect(),
        })
    }
}

/// Credit `links` on opportunity `id`, each board once (`now` for new ones).
async fn add_sources(
    conn: &mut PgConnection,
    id: Uuid,
    links: &[SourceLink],
    now: DateTime<Utc>,
) -> Result<()> {
    for link in links
        .iter()
        .filter_map(SourceLink::tidied)
        .take(MAX_SOURCES)
    {
        sqlx::query(
            "INSERT INTO pipeline.opportunity_sources (opportunity_id, board, name, url, seen_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (opportunity_id, board) DO NOTHING",
        )
        .bind(id)
        .bind(&link.board)
        .bind(&link.name)
        .bind(&link.url)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
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
             created_at, updated_at, remote, open_to_candidate, region_note, working_languages,
             apply_url, expires_at, canonical_url, dedupe_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16,
                 $17, $18, $19, $20, $21, $22, $23, $24)
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
    .bind(&row.apply_url)
    .bind(row.expires_at)
    .bind(&row.canonical_url)
    .bind(&row.dedupe_key)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(opportunity) = id {
        add_sources(conn, opportunity, &row.sources, now).await?;
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

/// The links and the company, role and place keys of `account`'s
/// opportunities (keys only of those added since `since`): a search
/// credits another board on them instead of adding them again.
pub async fn known_links(
    db: &PgPool,
    account: Uuid,
    since: DateTime<Utc>,
) -> Result<(HashSet<String>, HashSet<String>)> {
    let rows: Vec<(Option<String>, Option<String>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT canonical_url, dedupe_key, created_at FROM pipeline.opportunities
          WHERE account_id = $1 AND (canonical_url IS NOT NULL OR dedupe_key IS NOT NULL)",
    )
    .bind(account)
    .fetch_all(db)
    .await?;
    let mut urls = HashSet::new();
    let mut keys = HashSet::new();
    for (url, key, created_at) in rows {
        urls.extend(url);
        if created_at >= since {
            keys.extend(key);
        }
    }
    Ok((urls, keys))
}

/// A job seen on another board, to credit on the opportunity it matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attach {
    pub canonical_url: Option<String>,
    pub dedupe_key: Option<String>,
    pub sources: Vec<SourceLink>,
}

/// Credit each of `items`' boards on `account`'s opportunity it matches
/// (by link, else by company, role and place within 30 days). How many
/// matched.
pub async fn attach_sources(
    db: &PgPool,
    account: Uuid,
    items: Vec<Attach>,
    now: DateTime<Utc>,
) -> Result<u32> {
    let mut tx = db.begin().await?;
    let mut matched = 0_u32;
    for item in items {
        let id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM pipeline.opportunities
              WHERE account_id = $1
                AND (($2::text IS NOT NULL AND canonical_url = $2)
                     OR ($3::text IS NOT NULL AND dedupe_key = $3 AND created_at >= $4))
              ORDER BY (canonical_url = $2) DESC NULLS LAST, created_at
              LIMIT 1",
        )
        .bind(account)
        .bind(&item.canonical_url)
        .bind(&item.dedupe_key)
        .bind(now - TimeDelta::days(30))
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = id {
            add_sources(&mut tx, id, &item.sources, now).await?;
            matched += 1;
        }
    }
    tx.commit().await?;
    Ok(matched)
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

    #[sqlx::test(migrations = "../../../migrations")]
    async fn a_job_seen_on_another_board_is_credited_not_added(db: PgPool) {
        let account = Uuid::now_v7();
        let events = EventBus::default();
        let now = Utc::now();
        let link = |board: &str| SourceLink {
            board: board.into(),
            name: board.to_uppercase(),
            url: format!("https://{board}.example/jobs/1"),
        };
        let first = Found {
            source: "himalayas".into(),
            canonical_url: Some("https://jobs.example.com/apply/42".into()),
            dedupe_key: Some("example co|staff engineer|".into()),
            apply_url: Some("https://jobs.example.com/apply/42?utm_source=api".into()),
            sources: vec![
                link("himalayas"),
                link("remoteok"),
                SourceLink {
                    board: "evil".into(),
                    name: "Evil".into(),
                    url: "javascript:alert(1)".into(),
                },
            ],
            ..found("h:1")
        };
        assert_eq!(
            add_found(&db, &events, account, vec![first], now)
                .await
                .unwrap(),
            1
        );
        let (urls, keys) = known_links(&db, account, now - TimeDelta::days(30))
            .await
            .unwrap();
        assert!(urls.contains("https://jobs.example.com/apply/42"));
        assert!(keys.contains("example co|staff engineer|"));
        let matched = attach_sources(
            &db,
            account,
            vec![
                Attach {
                    canonical_url: None,
                    dedupe_key: Some("example co|staff engineer|".into()),
                    sources: vec![link("jobicy"), link("himalayas")],
                },
                Attach {
                    canonical_url: Some("https://elsewhere.example/1".into()),
                    dedupe_key: None,
                    sources: vec![link("remotive")],
                },
            ],
            now,
        )
        .await
        .unwrap();
        assert_eq!(matched, 1);
        let boards: Vec<String> = sqlx::query_scalar(
            "SELECT s.board FROM pipeline.opportunity_sources s
               JOIN pipeline.opportunities o ON o.id = s.opportunity_id
              WHERE o.account_id = $1 ORDER BY s.board",
        )
        .bind(account)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(boards, ["himalayas", "jobicy", "remoteok"]);
        assert!(is_search_source("we-work-remotely") && !is_search_source("link"));
        assert!(!is_search_source("LinkedIn"));
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
            source: "Linked In".into(),
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
