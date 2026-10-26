//! The usage ledger (`ai.usage`): one row per model call, with its tokens
//! and its cost in US dollars, per account. Ids, the role, the model,
//! counts and amounts only.
//!
//! Before a call the gateway sums the account's tokens for the UTC day
//! (the token cap) and, under one lock for the whole instance, its spend
//! for the day and the month and the instance's for the month: if the
//! call's worst case would cross a limit it is refused, otherwise that
//! worst case is **reserved** as a row. Once answered, the row is settled
//! at the real cost; a call that surely cost nothing (refused by the
//! provider, never reached it) is released. A call that timed out keeps
//! its worst case: it may have been charged.

use chrono::{DateTime, Datelike as _, NaiveDate, NaiveTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Role;

/// The start of `now`'s UTC day.
pub fn day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive().and_time(NaiveTime::MIN).and_utc()
}

/// The start of `now`'s calendar month, in UTC.
pub fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .unwrap_or_else(|| now.date_naive())
        .and_time(NaiveTime::MIN)
        .and_utc()
}

/// Which spending limit a call would cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The account's, for the UTC day.
    AccountDay,
    /// The account's, for the month.
    AccountMonth,
    /// The whole instance's, for the month.
    InstanceMonth,
}

impl Limit {
    /// Lower-case name, for logs and messages.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccountDay => "account_day",
            Self::AccountMonth => "account_month",
            Self::InstanceMonth => "instance_month",
        }
    }
}

/// The limits a call is checked against, in US dollars; `None` is none.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Limits {
    /// One account per UTC day.
    pub account_day: Option<f64>,
    /// One account per month.
    pub account_month: Option<f64>,
    /// The instance per month.
    pub instance_month: Option<f64>,
}

/// What has been spent, in US dollars, reservations included.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spent {
    /// By the account since the start of the UTC day.
    pub account_day: f64,
    /// By the account since the start of the month.
    pub account_month: f64,
    /// By everyone since the start of the month.
    pub instance_month: f64,
}

impl Spent {
    /// The first limit `cost` more would cross.
    pub fn crossed(&self, limits: &Limits, cost: f64) -> Option<Limit> {
        let over = |spent: f64, limit: Option<f64>| limit.is_some_and(|l| spent + cost > l);
        if over(self.instance_month, limits.instance_month) {
            Some(Limit::InstanceMonth)
        } else if over(self.account_month, limits.account_month) {
            Some(Limit::AccountMonth)
        } else if over(self.account_day, limits.account_day) {
            Some(Limit::AccountDay)
        } else {
            None
        }
    }
}

const SPENT: &str = "SELECT
        coalesce(sum(cost_usd) FILTER (WHERE account_id = $1 AND created_at >= $2), 0)::float8,
        coalesce(sum(cost_usd) FILTER (WHERE account_id = $1), 0)::float8,
        coalesce(sum(cost_usd), 0)::float8
   FROM ai.usage WHERE created_at >= $3";

/// What `account` and the instance spent by `now`.
pub async fn spent(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<Spent, sqlx::Error> {
    let (account_day, account_month, instance_month): (f64, f64, f64) = sqlx::query_as(SPENT)
        .bind(account)
        .bind(day_start(now))
        .bind(month_start(now))
        .fetch_one(db)
        .await?;
    Ok(Spent {
        account_day,
        account_month,
        instance_month,
    })
}

/// A reservation: the worst case of a call about to be sent.
#[derive(Debug, Clone, Copy)]
pub struct Reservation {
    /// The ledger row.
    pub id: Uuid,
    /// What was reserved, in US dollars.
    pub cost_usd: f64,
}

/// Reserve `cost_usd` for a call of `account` on `role`, unless it would
/// cross one of `limits`: `Ok(Err(limit))`, and nothing is written. Checks
/// and writes under one instance-wide lock, so concurrent calls cannot
/// both pass on the same remaining budget.
pub async fn reserve(
    db: &PgPool,
    account: Uuid,
    role: Role,
    model: &str,
    cost_usd: f64,
    limits: &Limits,
    now: DateTime<Utc>,
) -> Result<Result<Reservation, Limit>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('ai.usage.budget'))")
        .execute(&mut *tx)
        .await?;
    let (account_day, account_month, instance_month): (f64, f64, f64) = sqlx::query_as(SPENT)
        .bind(account)
        .bind(day_start(now))
        .bind(month_start(now))
        .fetch_one(&mut *tx)
        .await?;
    let spent = Spent {
        account_day,
        account_month,
        instance_month,
    };
    if let Some(limit) = spent.crossed(limits, cost_usd) {
        tx.rollback().await?;
        return Ok(Err(limit));
    }
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ai.usage (id, account_id, role, model, tokens_in, tokens_out, cost_usd,
                               settled, created_at)
         VALUES ($1, $2, $3, $4, 0, 0, $5, false, $6)",
    )
    .bind(id)
    .bind(account)
    .bind(role.as_str())
    .bind(model)
    .bind(cost_usd)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(Reservation { id, cost_usd }))
}

/// Settle `reservation` at what the call used and cost.
pub async fn settle(
    db: &PgPool,
    reservation: Reservation,
    model: &str,
    tokens_in: u32,
    tokens_out: u32,
    cost_usd: f64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ai.usage
            SET model = $2, tokens_in = $3, tokens_out = $4, cost_usd = $5, settled = true
          WHERE id = $1",
    )
    .bind(reservation.id)
    .bind(model)
    .bind(i32::try_from(tokens_in).unwrap_or(i32::MAX))
    .bind(i32::try_from(tokens_out).unwrap_or(i32::MAX))
    .bind(cost_usd.max(0.0))
    .execute(db)
    .await?;
    Ok(())
}

/// Keep `reservation`'s worst case: the call may have been charged (it
/// timed out, or its answer was unreadable).
pub async fn keep(db: &PgPool, reservation: Reservation) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ai.usage SET settled = true WHERE id = $1")
        .bind(reservation.id)
        .execute(db)
        .await?;
    Ok(())
}

/// Release `reservation`: the call cost nothing.
pub async fn release(db: &PgPool, reservation: Reservation) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM ai.usage WHERE id = $1")
        .bind(reservation.id)
        .execute(db)
        .await?;
    Ok(())
}

/// The instance's month so far: what it spent, in how many calls, for how
/// many accounts, and what of it today.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InstanceMonth {
    /// Spent this month, in US dollars.
    pub month_usd: f64,
    /// Spent today, in US dollars.
    pub today_usd: f64,
    /// Calls this month.
    pub calls: i64,
    /// Accounts that made one.
    pub accounts: i64,
}

/// The instance's spending for `now`'s month.
pub async fn instance_month(db: &PgPool, now: DateTime<Utc>) -> Result<InstanceMonth, sqlx::Error> {
    let (month_usd, today_usd, calls, accounts): (f64, f64, i64, i64) = sqlx::query_as(
        "SELECT coalesce(sum(cost_usd), 0)::float8,
                coalesce(sum(cost_usd) FILTER (WHERE created_at >= $2), 0)::float8,
                count(*), count(DISTINCT account_id)
           FROM ai.usage WHERE created_at >= $1",
    )
    .bind(month_start(now))
    .bind(day_start(now))
    .fetch_one(db)
    .await?;
    Ok(InstanceMonth {
        month_usd,
        today_usd,
        calls,
        accounts,
    })
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
    fn the_month_starts_on_the_first_at_utc_midnight() {
        let now = Utc.with_ymd_and_hms(2026, 10, 26, 17, 30, 5).unwrap();
        assert_eq!(
            month_start(now),
            Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn the_instance_limit_is_checked_first() {
        let limits = Limits {
            account_day: Some(0.3),
            account_month: Some(3.0),
            instance_month: Some(15.0),
        };
        let spent = Spent {
            account_day: 0.25,
            account_month: 1.0,
            instance_month: 14.99,
        };
        assert_eq!(spent.crossed(&limits, 0.01), None);
        assert_eq!(spent.crossed(&limits, 0.02), Some(Limit::InstanceMonth));
        let spent = Spent {
            instance_month: 0.0,
            ..spent
        };
        assert_eq!(spent.crossed(&limits, 0.06), Some(Limit::AccountDay));
        assert_eq!(spent.crossed(&Limits::default(), 1_000.0), None);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reservations_count_until_released(db: PgPool) {
        let now = Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
        let ana = Uuid::now_v7();
        let limits = Limits {
            account_day: Some(0.10),
            ..Limits::default()
        };
        let first = reserve(&db, ana, Role::Complete, "m", 0.06, &limits, now)
            .await
            .unwrap()
            .unwrap();
        // The first is still reserved at its worst case: the second would cross.
        assert_eq!(
            reserve(&db, ana, Role::Complete, "m", 0.06, &limits, now)
                .await
                .unwrap()
                .unwrap_err(),
            Limit::AccountDay
        );
        settle(&db, first, "m", 100, 10, 0.001).await.unwrap();
        let second = reserve(&db, ana, Role::Complete, "m", 0.06, &limits, now)
            .await
            .unwrap()
            .unwrap();
        release(&db, second).await.unwrap();
        let spent = spent(&db, ana, now).await.unwrap();
        assert!((spent.account_day - 0.001).abs() < 1e-9, "{spent:?}");
        assert!((spent.instance_month - 0.001).abs() < 1e-9);
        let month = instance_month(&db, now).await.unwrap();
        assert_eq!((month.calls, month.accounts), (1, 1));
    }

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
