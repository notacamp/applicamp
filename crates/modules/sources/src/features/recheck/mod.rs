//! The recheck: opportunities a search found before the requirement rules
//! get their eligibility facts once, from the same scoring call a search
//! makes, and the ones that fail a requirement are hidden with the reason.
//! Never deleted, and only while still Found or Shortlisted: later stages
//! are the person's own choice and are never touched.
//!
//! `sources.recheck_requirements` is enqueued at every server start (once:
//! a waiting one is not doubled). It is idempotent: it takes only
//! opportunities without facts, at most [`MOST`] a run, newest first, and
//! stays within each person's spending limits; what it could not check
//! waits for the next start.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use applicamp_ai::Caller;
use applicamp_kernel::{JobContext, JobError, Result, jobs};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    SourcesDeps,
    features::search::{configured, place_of, spend, spent_out},
    job::Job,
    ports::Unchecked,
    requirements,
    scoring::{self, BATCH},
};

/// Opportunities checked per run, at most.
pub const MOST: usize = 30;

/// Checks the requirements of opportunities found before the rules.
#[derive(Debug, Serialize, Deserialize)]
pub struct Recheck {}

impl applicamp_kernel::Job for Recheck {
    const NAME: &'static str = "sources.recheck_requirements";
    const QUEUE: &'static str = "searches";
    // Never retried: a retry could only spend again; the next start tries
    // what is left.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        recheck(deps, &ctx.db).await?;
        Ok(())
    }
}

/// Enqueue the recheck to run now, unless one is waiting. Whether it was.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &Recheck {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// What a recheck did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Rechecked {
    /// Opportunities that got their facts.
    pub checked: usize,
    /// Of those, the ones hidden for failing a requirement.
    pub hidden: usize,
}

/// Check up to [`MOST`] unchecked opportunities, person by person, ten a
/// model call, within each person's limits.
pub(crate) async fn recheck(deps: &SourcesDeps, db: &PgPool) -> Result<Rechecked> {
    let mut done = Rechecked::default();
    let (Some(neighbours), Some(ai)) = (&deps.neighbours, configured(deps)) else {
        return Ok(done);
    };
    let mut by_account: BTreeMap<Uuid, Vec<Unchecked>> = BTreeMap::new();
    for item in neighbours.unchecked(db, MOST).await? {
        by_account.entry(item.account).or_default().push(item);
    }
    for (account, items) in by_account {
        let now = deps.clock.now();
        let Some(input) = neighbours.matching_input(db, account, now).await? else {
            continue;
        };
        if spent_out(&spend(deps, db, account, now).await?) {
            continue;
        }
        for batch in items.chunks(BATCH) {
            let jobs: Vec<&Job> = batch.iter().map(|item| &item.job).collect();
            let caller = Caller {
                db,
                account,
                now: deps.clock.now(),
            };
            let scores = match scoring::score(ai, caller, &input, &jobs).await {
                Ok((scores, _)) => scores,
                Err(e) => {
                    tracing::warn!(error = %e, "a requirements check failed; the rest waits for the next start");
                    break;
                }
            };
            for (index, score) in scores {
                let item = &batch[index];
                let excluded =
                    requirements::check(&input, &score.facts, place_of(&item.job, &score)).excluded;
                let set = neighbours
                    .set_requirements(db, account, item.opportunity, score.facts, excluded)
                    .await?;
                if set {
                    done.checked += 1;
                    if excluded.is_some() {
                        done.hidden += 1;
                    }
                }
            }
        }
    }
    // Counts only.
    tracing::info!(
        checked = done.checked,
        hidden = done.hidden,
        "requirements checked"
    );
    Ok(done)
}
