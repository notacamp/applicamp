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
