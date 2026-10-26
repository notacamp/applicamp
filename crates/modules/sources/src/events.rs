//! The events the sources module publishes. Ids and counts only.

use applicamp_kernel::DomainEvent;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A search ended (finished, or stopped early).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFinished {
    /// Whose search.
    pub account: Uuid,
    /// The search.
    pub search: Uuid,
    /// Opportunities it added.
    pub added: u32,
}

impl DomainEvent for SearchFinished {
    const NAME: &'static str = "sources.search_finished";
    const VERSION: u16 = 1;
}
