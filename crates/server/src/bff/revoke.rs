//! Ending sessions when an account loses its invite.
//!
//! `bff.sessions` belongs to the BFF, not to identity, so identity only
//! announces `identity.invite_revoked` (in the transaction that takes the
//! address off the list) and this handler deletes the account's sessions.
//! Deleting is idempotent, so a retried delivery is harmless.

use applicamp_identity::events::InviteRevoked;
use applicamp_kernel::{
    Event, EventHandler,
    jobs::{JobContext, JobError},
};

use super::store;

/// Consumes [`InviteRevoked`]: every session of the account ends.
pub struct EndSessions;

impl EventHandler for EndSessions {
    type Event = InviteRevoked;
    const NAME: &'static str = "bff.end_sessions";

    async fn handle(ctx: JobContext, event: Event<InviteRevoked>) -> Result<(), JobError> {
        let ended = store::delete_for_account(&ctx.db, event.payload.account).await?;
        tracing::info!(account = %event.payload.account, ended, "ended the sessions of an uninvited account");
        Ok(())
    }
}
