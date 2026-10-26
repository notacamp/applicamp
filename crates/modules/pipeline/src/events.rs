//! The events the pipeline module publishes. Ids only.

use applicamp_kernel::DomainEvent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An opportunity joined someone's pipeline: found by a search, or added by
/// link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityAdded {
    /// Whose pipeline.
    pub account: Uuid,
    /// The opportunity.
    pub opportunity: Uuid,
}

impl DomainEvent for OpportunityAdded {
    const NAME: &'static str = "pipeline.opportunity_added";
    const VERSION: u16 = 1;
}
