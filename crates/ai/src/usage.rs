//! The usage ledger (`ai.usage`): one row per model call, with its tokens,
//! per account. Before a call the gateway sums the account's tokens for the
//! UTC day and refuses once they reach the cap; after it, it records the
//! call. Ids, the role, the model and counts only.

use chrono::{DateTime, NaiveTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Role;

/// The start of `now`'s UTC day.
pub fn day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive().and_time(NaiveTime::MIN).and_utc()
}

/// Tokens (in + out) `account` used since the start of `now`'s UTC day.
pub async fn used_today(
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let used: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(tokens_in::bigint + tokens_out::bigint), 0)::bigint
           FROM ai.usage WHERE account_id = $1 AND created_at >= $2",
    )
    .bind(account)
    .bind(day_start(now))
    .fetch_one(db)
    .await?;
    Ok(u64::try_from(used).unwrap_or(0))
}

/// One call's row.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// Whose call it was.
    pub account: Uuid,
    /// The role called.
    pub role: Role,
    /// The model that answered.
    pub model: &'a str,
    /// Prompt tokens.
    pub tokens_in: u32,
    /// Completion tokens.
    pub tokens_out: u32,
    /// When.
    pub at: DateTime<Utc>,
}

/// Record `entry`.
pub async fn record(db: &PgPool, entry: Entry<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ai.usage (id, account_id, role, model, tokens_in, tokens_out, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(entry.account)
    .bind(entry.role.as_str())
    .bind(entry.model)
    .bind(i32::try_from(entry.tokens_in).unwrap_or(i32::MAX))
    .bind(i32::try_from(entry.tokens_out).unwrap_or(i32::MAX))
    .bind(entry.at)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn the_day_starts_at_utc_midnight() {
        let now = Utc.with_ymd_and_hms(2026, 10, 26, 17, 30, 5).unwrap();
        assert_eq!(
            day_start(now),
            Utc.with_ymd_and_hms(2026, 10, 26, 0, 0, 0).unwrap()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn usage_is_summed_per_account_and_day(db: PgPool) {
        let now = Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
        let ana = Uuid::now_v7();
        let bo = Uuid::now_v7();
        for (account, tokens_in, at) in [
            (ana, 100, now),
            (ana, 50, now - chrono::TimeDelta::hours(2)),
            // Yesterday's does not count.
            (ana, 1_000, now - chrono::TimeDelta::days(1)),
            (bo, 7, now),
        ] {
            record(
                &db,
                Entry {
                    account,
                    role: Role::Complete,
                    model: "m",
                    tokens_in,
                    tokens_out: 10,
                    at,
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(used_today(&db, ana, now).await.unwrap(), 170);
        assert_eq!(used_today(&db, bo, now).await.unwrap(), 17);
        assert_eq!(used_today(&db, Uuid::now_v7(), now).await.unwrap(), 0);
    }
}
