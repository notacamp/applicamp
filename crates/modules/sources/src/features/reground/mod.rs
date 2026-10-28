//! The one-off reason check: opportunities a search found before reasons
//! were grounded (#27) have their reasons checked against the profile
//! once. A reason that claims something the profile and goals do not say
//! ("Full-stack TypeScript matches profile" for someone who lists no
//! skills) is stripped, and the peek says "Reasons updated". The
//! opportunity stays as it is: never hidden, never deleted, its stage, its
//! match and its notes untouched, and only while it is still Found or
//! Shortlisted (later stages are the person's own choice).
//!
//! `sources.reground_reasons` is enqueued at every server start (once: a
//! waiting one is not doubled). It is idempotent and costs nothing (no
//! model call): it takes only opportunities not yet checked, a page of
//! [`PAGE`] at a time by id, and marks each one it handled. Someone whose
//! profile is gone waits for a later start.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use applicamp_kernel::{JobContext, JobError, Result, jobs};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{SourcesDeps, grounding::Source, scoring::source_of};

/// Opportunities handled per page.
pub const PAGE: usize = 200;

/// Checks the reasons of opportunities found before the check.
#[derive(Debug, Serialize, Deserialize)]
pub struct Reground {}

impl applicamp_kernel::Job for Reground {
    const NAME: &'static str = "sources.reground_reasons";
    const QUEUE: &'static str = "searches";
    // Idempotent and free: a retry is safe, but the next start also tries.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        reground(deps, &ctx.db).await?;
        Ok(())
    }
}

/// Enqueue the check to run now, unless one is waiting. Whether it was.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &Reground {}, deps.clock.now()).await?;
    tx.commit().await?;
    Ok(added)
}

/// What a check did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Regrounded {
    /// Opportunities whose reasons were checked.
    pub checked: usize,
    /// Of those, the ones that lost a reason.
    pub updated: usize,
}

/// Check the reasons of every unchecked opportunity, person by person.
pub(crate) async fn reground(deps: &SourcesDeps, db: &PgPool) -> Result<Regrounded> {
    let mut done = Regrounded::default();
    let Some(neighbours) = &deps.neighbours else {
        return Ok(done);
    };
    let mut sources: HashMap<Uuid, Option<Source>> = HashMap::new();
    let mut after = Uuid::nil();
    loop {
        let page = neighbours.unreasoned(db, after, PAGE).await?;
        let Some(last) = page.last() else {
            break;
        };
        after = last.opportunity;
        for item in page {
            if !sources.contains_key(&item.account) {
                let input = neighbours
                    .matching_input(db, item.account, deps.clock.now())
                    .await?;
                sources.insert(item.account, input.as_ref().map(source_of));
            }
            let Some(Some(source)) = sources.get(&item.account) else {
                continue;
            };
            let (kept, dropped) = source.keep(item.why);
            let set = neighbours
                .set_reasons(db, item.account, item.opportunity, kept, dropped > 0)
                .await?;
            if set {
                done.checked += 1;
                if dropped > 0 {
                    done.updated += 1;
                }
            }
        }
    }
    // Counts only.
    tracing::info!(
        checked = done.checked,
        updated = done.updated,
        "reasons checked"
    );
    Ok(done)
}
