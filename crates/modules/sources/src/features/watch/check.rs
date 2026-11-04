//! The daily check: each watched company's job board is read, compared with
//! the postings already seen, and only the new ones go on to the search's
//! hard limits and scoring (so most days there is nothing to score, and no
//! model is called).
//!
//! It runs inside the search, the nightly one at 21:00 Lisbon time or
//! "Search now" (see `design.md` of the `company-watchers` change): one
//! batch, one cost line, the same spending limits. A board is read at most
//! once per [`crate::SourcesConfig::watch_every`]. A board that fails keeps
//! its watch and says why; it is not asked again until the next day.

use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use applicamp_kernel::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    SourcesDeps,
    job::Job,
    readers::{Board, FetchError, Outcome, Reader},
};

use super::{
    detect::{self, BoardKind, Who},
    store::{self, Row},
};

/// New postings of one watched board scored in one search, at most (the rest
/// wait for the next).
pub const MOST_NEW: usize = 20;

/// How reading one watched board went.
#[derive(Debug, Clone)]
pub struct Read {
    pub watch: Uuid,
    pub company: Uuid,
    pub name: String,
    pub outcome: Outcome,
    /// What went wrong ([`FetchError::kind`] or a reason).
    pub error: Option<String>,
    pub duration_ms: u32,
    /// Jobs the board lists.
    pub listed: u32,
    /// Of them, new since the last read.
    pub new: u32,
}

/// What the watched boards gave a search.
#[derive(Debug, Default)]
pub struct Reading {
    /// The new postings, each once.
    pub jobs: Vec<Job>,
    /// The watch each new posting is of.
    pub of: HashMap<(String, String), Uuid>,
    /// The company each watch is of.
    pub company_of: HashMap<Uuid, (Uuid, String)>,
    /// Each board read.
    pub reads: Vec<Read>,
}

/// The jobs on `row`'s board.
async fn jobs_of(
    reader: &Reader,
    db: &PgPool,
    row: &Row,
) -> std::result::Result<Vec<Job>, FetchError> {
    let who = Who {
        id: row.company,
        name: row.name.clone(),
    };
    match row.kind() {
        BoardKind::Greenhouse
        | BoardKind::Lever
        | BoardKind::Ashby
        | BoardKind::Workable
        | BoardKind::Recruitee
        | BoardKind::Teamtailor
        | BoardKind::Personio => {
            let (Some(source), Some(token)) = (row.kind().source(), row.board_token.clone()) else {
                return Err(FetchError::BadUrl);
            };
            reader
                .board(
                    db,
                    &Board {
                        kind: source,
                        token,
                        company: row.name.clone(),
                        eu: row.board_eu,
                    },
                )
                .await
        }
        BoardKind::Jsonld => {
            let url = row
                .careers_url
                .as_deref()
                .and_then(|u| Url::parse(u).ok())
                .ok_or(FetchError::BadUrl)?;
            detect::read_careers(reader, &who, &url).await
        }
        BoardKind::None => Err(FetchError::BadUrl),
    }
}

/// Read the boards of `account`'s watches that are due, and give the postings
/// they have not seen. A board's first read after it was found only notes
/// what is there (the watch is "from today").
pub async fn read_due(
    deps: &SourcesDeps,
    reader: &Reader,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Reading> {
    let mut reading = Reading::default();
    let rows = store::due(db, account, now - deps.config.watch_every()).await?;
    for row in rows {
        let started = Instant::now();
        let result = jobs_of(reader, db, &row).await;
        let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        reading
            .company_of
            .insert(row.watch, (row.company, row.name.clone()));
        match result {
            Ok(jobs) => {
                let mut listed: Vec<(String, String)> = Vec::new();
                let mut seen_here = HashSet::new();
                let jobs: Vec<Job> = jobs
                    .into_iter()
                    .filter(|job| seen_here.insert(job.key()))
                    .collect();
                listed.extend(jobs.iter().map(Job::key));
                let seen = store::seen(db, row.watch).await?;
                let first_read = row.seeded_at.is_none();
                let new: Vec<Job> = if first_read {
                    Vec::new()
                } else {
                    jobs.into_iter()
                        .filter(|job| !seen.contains(&job.key()))
                        .collect()
                };
                if first_read {
                    let mut tx = db.begin().await?;
                    store::note_seen(&mut tx, row.watch, &listed, now).await?;
                    tx.commit().await?;
                }
                store::read_done(db, row.watch, &listed, None, now).await?;
                // A company page of the registry keeps what its watcher read
                // (searches read company pages of watched companies when
                // `APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES` is on).
                if deps.config.search_company_pages()
                    && let Some(page) = row.page_id
                    && let Err(e) =
                        crate::company_pages::note_read(db, page, listed.len(), now).await
                {
                    tracing::warn!(error = %e, "a company page's reading could not be kept");
                }
                reading.reads.push(Read {
                    watch: row.watch,
                    company: row.company,
                    name: row.name.clone(),
                    outcome: Outcome::Read,
                    error: None,
                    duration_ms,
                    listed: u32::try_from(listed.len()).unwrap_or(u32::MAX),
                    new: u32::try_from(new.len()).unwrap_or(u32::MAX),
                });
                for job in new.into_iter().take(MOST_NEW) {
                    reading.of.insert(job.key(), row.watch);
                    reading.jobs.push(job);
                }
            }
            Err(e) => {
                let kind = e.kind();
                // The board and the error only: no URL, no answer.
                tracing::info!(kind = row.kind().as_str(), error = %kind, "a watched board could not be read");
                store::read_done(db, row.watch, &[], Some(&kind), now).await?;
                reading.reads.push(Read {
                    watch: row.watch,
                    company: row.company,
                    name: row.name.clone(),
                    outcome: Outcome::Error,
                    error: Some(kind),
                    duration_ms,
                    listed: 0,
                    new: 0,
                });
            }
        }
    }
    Ok(reading)
}

/// Note the postings of `watch` that a search is done with (scored, or left
/// out by a hard limit, or already in the pipeline) as seen. The ones it
/// could not score (the spending limit) are left to the next search.
pub async fn mark_seen(
    db: &PgPool,
    watch: Uuid,
    keys: &[(String, String)],
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    store::note_seen(&mut tx, watch, keys, now).await?;
    tx.commit().await?;
    Ok(())
}
