//! The session store (schema `bff`). A cookie holds a random token; only
//! its SHA-256 is stored, so a database leak opens no session.

use applicamp_kernel::crypto::{hash, random_token};
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// How long a session lasts after its last use.
pub const LIFETIME: TimeDelta = TimeDelta::days(30);

/// Record use of a session (and move its expiry) at most this often.
pub const TOUCH_EVERY: TimeDelta = TimeDelta::hours(1);

/// A live session.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Session {
    /// The row.
    pub id: Uuid,
    /// Who it signs in.
    pub account_id: Uuid,
    /// When it was last used.
    pub last_seen_at: DateTime<Utc>,
}

/// Start a session for `account`. Its id and the token for the cookie.
pub async fn create(
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<(Uuid, String), sqlx::Error> {
    let id = Uuid::now_v7();
    let token = random_token();
    sqlx::query(
        "INSERT INTO bff.sessions (id, token_hash, account_id, created_at, last_seen_at, expires_at)
         VALUES ($1, $2, $3, $4, $4, $5)",
    )
    .bind(id)
    .bind(hash(&token))
    .bind(account)
    .bind(now)
    .bind(now + LIFETIME)
    .execute(db)
    .await?;
    Ok((id, token))
}

/// The live session `token` opens.
pub async fn find(
    db: &PgPool,
    token: &str,
    now: DateTime<Utc>,
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, last_seen_at FROM bff.sessions
          WHERE token_hash = $1 AND expires_at > $2",
    )
    .bind(hash(token))
    .bind(now)
    .fetch_optional(db)
    .await
}

/// Record that session `id` was used at `now`: it lasts [`LIFETIME`] more.
pub async fn touch(db: &PgPool, id: Uuid, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE bff.sessions SET last_seen_at = $2, expires_at = $3 WHERE id = $1")
        .bind(id)
        .bind(now)
        .bind(now + LIFETIME)
        .execute(db)
        .await?;
    Ok(())
}

/// End the session `token` opens. Whether there was one.
pub async fn delete(db: &PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM bff.sessions WHERE token_hash = $1")
        .bind(hash(token))
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Delete ended sessions. How many.
pub async fn prune(db: &PgPool, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let done = sqlx::query("DELETE FROM bff.sessions WHERE expires_at <= $1")
        .bind(now)
        .execute(db)
        .await?;
    Ok(done.rows_affected())
}
