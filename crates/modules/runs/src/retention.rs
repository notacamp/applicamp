//! Run events are kept 30 days: a daily job deletes the events of runs
//! that finished longer ago. The runs keep their facts and summaries.

use applicamp_kernel::jobs::{self, Job, JobContext, JobError};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::store;

/// How long a finished run's events are kept.
pub const RETENTION: TimeDelta = TimeDelta::days(30);
/// How often old events are deleted.
pub const EVERY: TimeDelta = TimeDelta::days(1);

/// Delete old runs' events, then schedule the next pass.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneEvents {}

impl Job for PruneEvents {
    const NAME: &'static str = "runs.prune_events";

    async fn run(self, ctx: JobContext) -> Result<(), JobError> {
        let now = Utc::now();
        let pruned = store::prune_events(&ctx.db, now - RETENTION).await?;
        if pruned > 0 {
            tracing::info!(pruned, "deleted old run events");
        }
        let mut tx = ctx.db.begin().await?;
        // This job is still running, so a unique enqueue would see it.
        jobs::enqueue_at(&mut tx, &Self {}, now + EVERY).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Make sure a pass is scheduled. Whether one was added.
pub async fn schedule(db: &PgPool) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let added =
        jobs::enqueue_unique_at(&mut tx, &PruneEvents {}, Utc::now() + TimeDelta::minutes(5))
            .await?;
    tx.commit().await?;
    Ok(added)
}
