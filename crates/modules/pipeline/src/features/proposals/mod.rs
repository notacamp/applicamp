//! Shortlist proposals (#73, ADR 0010).
//!
//! A job a search finds that fits well (a match of 75 or more, open to the
//! person, not hidden) is **proposed**: `GET /api/v1/opportunities?proposed=true`
//! lists them, best match first ([`crate::store::add_found`] marks them;
//! moving the stage or setting the job aside clears it).
//!
//! When a search ends (`sources.search_finished`), and the person's Autonomy
//! goal is "Prepare" or not answered, the app shortlists the best proposals
//! by itself: at most [`PER_DAY`] a UTC day per account, never a posting
//! that has closed, and never without room under the daily CV draft cap and
//! the account's and the instance's AI spending limits for one more
//! preparation. Shortlisting starts the preparation of #70, which waits on
//! the person for every approval. With "Find only", nothing moves. No model
//! call here.

#[cfg(test)]
mod tests;

use applicamp_kernel::{
    DomainEvent, Event, EventHandler, Job, Result,
    jobs::{self, JobContext, JobError},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    PipelineDeps,
    features::{brief, cv, prepare},
};

/// The most jobs the app shortlists by itself per UTC day per account.
pub const PER_DAY: i64 = 3;

/// `sources.search_finished`, as the pipeline reads it: ids and counts only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFinished {
    /// Whose search.
    pub account: Uuid,
    /// The search.
    pub search: Uuid,
    /// Opportunities it added.
    pub added: u32,
}

impl DomainEvent for SearchFinished {
    const NAME: &'static str = "sources.search_finished";
    const VERSION: u16 = 1;
}

/// After a search: queue [`ShortlistProposals`] for its person.
pub struct AfterSearch;

impl EventHandler for AfterSearch {
    type Event = SearchFinished;
    const NAME: &'static str = "pipeline.after_search";

    async fn handle(ctx: JobContext, event: Event<SearchFinished>) -> Result<(), JobError> {
        let mut conn = ctx.db.acquire().await?;
        jobs::enqueue(
            &mut conn,
            &ShortlistProposals {
                account: event.payload.account,
            },
        )
        .await?;
        Ok(())
    }
}

/// Shortlist the best proposals of `account` by itself, when they let it.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShortlistProposals {
    pub account: Uuid,
}

impl Job for ShortlistProposals {
    const NAME: &'static str = "pipeline.shortlist_proposals";
    // A failure is not retried by itself: the next search tries again.
    const MAX_ATTEMPTS: u32 = 1;

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<PipelineDeps>()
            .ok_or_else(|| JobError::Permanent("the pipeline module is not composed".into()))?;
        shortlist(deps, &ctx.db, self.account, Utc::now())
            .await
            .map_err(|e| JobError::Permanent(e.code().to_owned()))?;
        Ok(())
    }
}

/// Why the app shortlists nothing by itself now, as a code; none when it may.
/// It may only when the account and the instance have room for one more
/// preparation at its worst (reading the company, comparing the job and
/// drafting the CV).
async fn held(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<&'static str>> {
    if !prepare::by_itself(deps, db, account).await? {
        return Ok(Some("find_only"));
    }
    if cv::drafts_today(db, account, now).await? >= cv::CAP_PER_DAY {
        return Ok(Some("cv_drafts_daily_cap"));
    }
    if let Some(ai) = &deps.ai {
        let one = brief::COMPANY_READ_EUR + brief::ANALYSIS_EUR + cv::draft_eur(ai);
        let over = |spent: f64, limit: Option<f64>| limit.is_some_and(|limit| spent + one > limit);
        let spend = ai.spend(db, account, now).await?;
        if over(spend.today_eur, spend.day_limit_eur) {
            return Ok(Some("ai_daily_limit_reached"));
        }
        if over(spend.month_eur, spend.month_limit_eur) {
            return Ok(Some("ai_budget_reached"));
        }
        let instance = ai.instance_spend(db, now).await?;
        if over(instance.month_eur, instance.limit_eur) {
            return Ok(Some("ai_budget_reached"));
        }
    }
    Ok(None)
}

/// Shortlist `account`'s best proposals by itself, within [`PER_DAY`], and
/// start their preparation. How many. The count, the pick and the moves run
/// in one transaction under a lock per account, so two searches ending at
/// once cannot shortlist more than [`PER_DAY`]; a posting that has closed is
/// never picked.
pub(crate) async fn shortlist(
    deps: &PipelineDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<u32> {
    if let Some(why) = held(deps, db, account, now).await? {
        tracing::info!(reason = why, "no job shortlisted by the app");
        return Ok(0);
    }
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('pipeline.shortlist:' || $1::text))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let today: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.opportunities
          WHERE account_id = $1 AND shortlisted_by = 'app' AND shortlisted_at >= $2",
    )
    .bind(account)
    .bind(applicamp_ai::usage::day_start(now))
    .fetch_one(&mut *tx)
    .await?;
    let room = PER_DAY - today;
    if room <= 0 {
        return Ok(0);
    }
    let picked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM pipeline.opportunities
          WHERE account_id = $1 AND proposed_at IS NOT NULL AND stage = 'found'
            AND NOT dismissed AND hidden_reason IS NULL
            AND open_to_candidate IS DISTINCT FROM 'no'
            AND (expires_at IS NULL OR expires_at >= $3)
          ORDER BY match_score DESC NULLS LAST, created_at DESC, id
          LIMIT $2",
    )
    .bind(account)
    .bind(room)
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    let mut moved: Vec<Uuid> = Vec::new();
    for id in picked {
        let done = sqlx::query(
            "UPDATE pipeline.opportunities
                SET stage = 'shortlisted', shortlisted_by = 'app', shortlisted_at = $3,
                    proposed_at = NULL, updated_at = $3
              WHERE id = $1 AND account_id = $2 AND stage = 'found' AND NOT dismissed",
        )
        .bind(id)
        .bind(account)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if done > 0 {
            moved.push(id);
        }
    }
    tx.commit().await?;
    for &id in &moved {
        prepare::begin(deps, db, account, id, None, now).await;
        // What the app did, on the job's preparation: shortlisted it.
        prepare::record(db, id, "shortlist", 0.0, now).await?;
    }
    let done = u32::try_from(moved.len()).unwrap_or(u32::MAX);
    if done > 0 {
        tracing::info!(shortlisted = done, "jobs shortlisted by the app");
    }
    Ok(done)
}
