//! The invite list: who may create an account. Changed only from the
//! server's command line (`applicamp invite add|remove|list`); no HTTP
//! endpoint reads or changes it.

#[cfg(test)]
mod tests;

use applicamp_kernel::{Error, Result, events::EventBus};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::events::InviteRevoked;

/// Longest address accepted (RFC 5321's path limit).
const MAX_EMAIL_LEN: usize = 254;

/// One invited address.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Invite {
    /// The address, lower-cased.
    pub email: String,
    /// When it was added.
    pub invited_at: DateTime<Utc>,
    /// Why, for the operator.
    pub note: Option<String>,
}

/// What [`add`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// The address is on the list now.
    New,
    /// It already was; nothing changed.
    AlreadyInvited,
}

/// `raw` as stored: trimmed and lower-cased, if it looks like an address
/// (one `@` with something on both sides, a dot in the domain, no spaces).
pub fn normalize_email(raw: &str) -> Result<String> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= MAX_EMAIL_LEN
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(Error::BadRequest(
            "invalid_email",
            "That is not an email address.".to_owned(),
        ))
    }
}

/// Put `email` on the list.
pub async fn add(
    db: &PgPool,
    email: &str,
    note: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Added> {
    let email = normalize_email(email)?;
    let inserted = sqlx::query(
        "INSERT INTO identity.invites (email, invited_at, note) VALUES ($1, $2, $3)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(email.as_str())
    .bind(now)
    .bind(note)
    .execute(db)
    .await?;
    Ok(if inserted.rows_affected() == 1 {
        Added::New
    } else {
        Added::AlreadyInvited
    })
}

/// Take `email` off the list and withdraw access from the account with that
/// address, if any: [`InviteRevoked`] is published in the same transaction,
/// so every session of the account ends. The account, its ways in and its
/// data are kept; inviting the address again restores access. Whether it was
/// on the list.
///
/// The event is published even when the address was not on the list but an
/// account has it, so running the command again finishes an interrupted
/// revocation.
pub async fn remove(db: &PgPool, events: &EventBus, email: &str) -> Result<bool> {
    let email = normalize_email(email)?;
    let mut tx = db.begin().await?;
    let removed = sqlx::query("DELETE FROM identity.invites WHERE email = $1")
        .bind(email.as_str())
        .execute(&mut *tx)
        .await?;
    let account: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM identity.accounts WHERE email = $1")
            .bind(email.as_str())
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(account) = account {
        events.publish(&mut tx, &InviteRevoked { account }).await?;
    }
    tx.commit().await?;
    Ok(removed.rows_affected() == 1)
}

/// The whole list, oldest first.
pub async fn list(db: &PgPool) -> Result<Vec<Invite>> {
    Ok(sqlx::query_as(
        "SELECT email, invited_at, note FROM identity.invites ORDER BY invited_at, email",
    )
    .fetch_all(db)
    .await?)
}
