//! The invite list: who may create an account. Changed only from the
//! server's command line (`applicamp invite add|remove|list`); no HTTP
//! endpoint reads or changes it.

#[cfg(test)]
mod tests;

use applicamp_kernel::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

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

/// Take `email` off the list. Whether it was on it. Accounts already
/// created keep their ways in.
pub async fn remove(db: &PgPool, email: &str) -> Result<bool> {
    let email = normalize_email(email)?;
    let removed = sqlx::query("DELETE FROM identity.invites WHERE email = $1")
        .bind(email.as_str())
        .execute(db)
        .await?;
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
