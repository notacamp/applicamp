//! The events the identity module publishes. Ids only.

use applicamp_kernel::DomainEvent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A first sign-in created an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCreated {
    /// The new account.
    pub account: Uuid,
}

impl DomainEvent for AccountCreated {
    const NAME: &'static str = "identity.account_created";
    const VERSION: u16 = 1;
}

/// An address left the invite list while an account has it: that account's
/// access is withdrawn (its sessions end). The account and its data stay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteRevoked {
    /// The account whose address was taken off the list.
    pub account: Uuid,
}

impl DomainEvent for InviteRevoked {
    const NAME: &'static str = "identity.invite_revoked";
    const VERSION: u16 = 1;
}
