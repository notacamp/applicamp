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

/// Someone's goals changed. Ids and a version number, never the answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalsUpdated {
    /// Whose goals.
    pub account: Uuid,
    /// The version they are now at (agents record the one they used).
    pub version: i32,
}

impl DomainEvent for GoalsUpdated {
    const NAME: &'static str = "profile.goals_updated";
    const VERSION: u16 = 1;
}
