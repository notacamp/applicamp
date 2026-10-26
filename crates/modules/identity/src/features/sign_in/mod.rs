//! Signing in: which account a provider identity is, if any.
//!
//! The rule (`openspec/specs/identity`):
//! 1. A way in already linked (same provider and subject) signs its account in.
//! 2. Else a **verified** address on the **invite list** that is an
//!    account's address links this way in to that account.
//! 3. Else a verified address on the invite list creates an account with
//!    this way in.
//! 4. Anything else is "not on the list yet": nothing is created.

#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Result, crypto::Cipher};
use secrecy::{ExposeSecret as _, SecretString};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Provider, events::AccountCreated, invites::normalize_email};

/// The key purpose sealing stored refresh tokens. Never change it.
const TOKEN_PURPOSE: &[u8] = b"applicamp identity ways_in refresh_token v1";

/// Who a provider says signed in, after the ID token validated.
pub struct ProviderIdentity {
    /// The provider.
    pub provider: Provider,
    /// The provider's `sub`.
    pub subject: String,
    /// The address the provider gives.
    pub email: Option<String>,
    /// Whether the provider verified it.
    pub email_verified: bool,
    /// The person's name.
    pub name: Option<String>,
    /// Their photo.
    pub picture_url: Option<String>,
    /// A refresh token to keep (Campbooks only; others are dropped).
    pub refresh_token: Option<SecretString>,
}

impl std::fmt::Debug for ProviderIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No address, name or token: this may reach a log.
        f.debug_struct("ProviderIdentity")
            .field("provider", &self.provider)
            .field("email_verified", &self.email_verified)
            .field("refresh_token", &self.refresh_token.is_some())
            .finish_non_exhaustive()
    }
}

/// What signing in decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignIn {
    /// Signed in to this account.
    Account(Uuid),
    /// Not on the invite list: no account was created or changed.
    NotInvited,
}

/// The cipher sealing refresh tokens under the instance secret.
pub fn token_cipher(state: &AppState) -> Cipher {
    Cipher::new(&state.secret, TOKEN_PURPOSE)
}

/// Associated data binding a sealed refresh token to its way in.
pub fn token_context(way_in: Uuid) -> Vec<u8> {
    format!("identity.ways_in:refresh_token:{way_in}").into_bytes()
}

/// Decide which account `who` signs in to (creating or linking one as the
/// rule allows), in one transaction.
pub async fn sign_in(state: &AppState, who: ProviderIdentity) -> Result<SignIn> {
    let now = state.clock.now();
    let cipher = token_cipher(state);
    let mut tx = state.db.begin().await?;

    let linked: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, account_id FROM identity.ways_in
          WHERE provider = $1 AND subject = $2
            FOR UPDATE",
    )
    .bind(who.provider.as_str())
    .bind(who.subject.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let (way_in, account) = match linked {
        Some(found) => found,
        None => {
            let Some(email) = invited_address(&mut tx, &who).await? else {
                tx.rollback().await?;
                tracing::info!(provider = %who.provider, "sign-in refused: not on the invite list");
                return Ok(SignIn::NotInvited);
            };
            let account = match account_by_email(&mut tx, &email).await? {
                Some(account) => account,
                None => {
                    let account = create_account(&mut tx, &who, &email, now).await?;
                    state
                        .events
                        .publish(&mut tx, &AccountCreated { account })
                        .await?;
                    account
                }
            };
            let way_in = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO identity.ways_in
                     (id, account_id, provider, subject, created_at, last_used_at)
                 VALUES ($1, $2, $3, $4, $5, $5)",
            )
            .bind(way_in)
            .bind(account)
            .bind(who.provider.as_str())
            .bind(who.subject.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
            tracing::info!(provider = %who.provider, "linked a way in");
            (way_in, account)
        }
    };

    let sealed = who
        .refresh_token
        .as_ref()
        .filter(|_| who.provider == Provider::Campbooks)
        .map(|token| cipher.seal(token.expose_secret(), &token_context(way_in)));
    sqlx::query(
        "UPDATE identity.ways_in
            SET last_used_at = $2, refresh_token = coalesce($3, refresh_token)
          WHERE id = $1",
    )
    .bind(way_in)
    .bind(now)
    .bind(sealed)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE identity.accounts SET last_sign_in_at = $2 WHERE id = $1")
        .bind(account)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(SignIn::Account(account))
}

/// `who`'s address, normalized, when it is verified and on the invite list.
async fn invited_address(
    conn: &mut PgConnection,
    who: &ProviderIdentity,
) -> Result<Option<String>> {
    let Some(email) = who
        .email
        .as_deref()
        .filter(|_| who.email_verified)
        .and_then(|email| normalize_email(email).ok())
    else {
        return Ok(None);
    };
    let invited: Option<String> =
        sqlx::query_scalar("SELECT email FROM identity.invites WHERE email = $1")
            .bind(email.as_str())
            .fetch_optional(conn)
            .await?;
    Ok(invited)
}

async fn account_by_email(conn: &mut PgConnection, email: &str) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM identity.accounts WHERE email = $1 FOR UPDATE")
            .bind(email)
            .fetch_optional(conn)
            .await?,
    )
}

async fn create_account(
    conn: &mut PgConnection,
    who: &ProviderIdentity,
    email: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO identity.accounts (id, email, email_verified, name, picture_url, created_at)
         VALUES ($1, $2, true, $3, $4, $5)",
    )
    .bind(id)
    .bind(email)
    .bind(who.name.as_deref())
    .bind(who.picture_url.as_deref())
    .bind(now)
    .execute(conn)
    .await?;
    tracing::info!(provider = %who.provider, "created an account");
    Ok(id)
}
