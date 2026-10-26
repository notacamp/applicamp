//! The events the profile module publishes. Ids only, never profile content.

use applicamp_kernel::DomainEvent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Someone's profile changed: an import was applied, or they edited it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUpdated {
    /// Whose profile.
    pub account: Uuid,
}

impl DomainEvent for ProfileUpdated {
    const NAME: &'static str = "profile.updated";
    const VERSION: u16 = 1;
}
