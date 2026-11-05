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
//!
//! It also shows again, with no model call, the opportunities hidden only
//! because their posting did not say whether it hires where the person
//! lives (#74): the rules are applied again to the facts kept on them (and
//! the pay last read for the job, when a cached score has it); one that now
//! passes is back in view with the note "Doesn't say if it hires in
//! Portugal", one that fails another rule stays hidden.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use applicamp_ai::Caller;
use applicamp_kernel::{JobContext, JobError, Result, jobs};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    Neighbours, SourcesDeps,
    features::search::{configured, place_of, spend, spent_out},
    job::Job,
    ports::{HiddenUnclear, Unchecked},
    requirements::{self, Answer, Salary},
    scoring::{self, BATCH},
};

/// Opportunities checked per run, at most.
pub const MOST: usize = 30;
/// Hidden opportunities read per page when showing them again.
const PAGE: usize = 200;

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
    /// Opportunities hidden only because their posting did not say where it
    /// hires, shown again with that note.
    pub shown: usize,
}

/// Check up to [`MOST`] unchecked opportunities, person by person, ten a
/// model call, within each person's limits; then show again the ones hidden
/// only for not saying where they hire.
pub(crate) async fn recheck(deps: &SourcesDeps, db: &PgPool) -> Result<Rechecked> {
    let mut done = Rechecked::default();
    let Some(neighbours) = &deps.neighbours else {
        return Ok(done);
    };
    if let Some(ai) = configured(deps) {
        check_unchecked(deps, neighbours.as_ref(), ai, db, &mut done).await?;
    }
    done.shown = show_again(deps, neighbours.as_ref(), db).await?;
    // Counts only.
    tracing::info!(
        checked = done.checked,
        hidden = done.hidden,
        shown = done.shown,
        "requirements checked"
    );
    Ok(done)
}

/// Give up to [`MOST`] opportunities without facts their facts.
async fn check_unchecked(
    deps: &SourcesDeps,
    neighbours: &dyn Neighbours,
    ai: &applicamp_ai::Gateway,
    db: &PgPool,
    done: &mut Rechecked,
) -> Result<()> {
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
    Ok(())
}

/// The pay last read for `item`'s job for its person, when a cached full
/// read has it.
async fn pay_of(db: &PgPool, item: &HiddenUnclear) -> Result<Option<Salary>> {
    let salary: Option<Option<sqlx::types::Json<Salary>>> = sqlx::query_scalar(
        "SELECT salary FROM sources.scores
          WHERE account_id = $1 AND source = $2 AND source_ref = $3 AND stage = 'full'
            AND salary IS NOT NULL
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(item.account)
    .bind(&item.source)
    .bind(&item.source_ref)
    .fetch_optional(db)
    .await?;
    Ok(salary.flatten().map(|s| s.0))
}

/// Show again the opportunities hidden only because their posting did not
/// say whether it hires where the person lives, with that note; how many.
async fn show_again(deps: &SourcesDeps, neighbours: &dyn Neighbours, db: &PgPool) -> Result<usize> {
    let mut shown = 0;
    let mut after = Uuid::nil();
    loop {
        let page = neighbours.hidden_unclear(db, after, PAGE).await?;
        let Some(last) = page.last() else {
            break;
        };
        after = last.opportunity;
        let last_page = page.len() < PAGE;
        for item in page {
            // Only what the posting left open is judged again.
            if item.facts.open_to_candidate != Answer::Unclear {
                continue;
            }
            let Some(input) = neighbours
                .matching_input(db, item.account, deps.clock.now())
                .await?
            else {
                continue;
            };
            let mut facts = item.facts.clone();
            facts.salary = pay_of(db, &item).await?;
            let verdict = requirements::check(&input, &facts, item.location.as_deref());
            if verdict.excluded.is_some() {
                continue;
            }
            if neighbours
                .show_with_gaps(db, item.account, item.opportunity, verdict.gaps)
                .await?
            {
                shown += 1;
            }
        }
        if last_page {
            break;
        }
    }
    Ok(shown)
}
