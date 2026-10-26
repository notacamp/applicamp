//! `applicamp-pipeline`: every opportunity a person has, in one place.
//!
//! - **Opportunities**: found by a search (the `sources` module adds them as
//!   Found, through the port the server wires) or added by pasting a link.
//!   Each has a stage (Found, Shortlisted, Applying, Applied, Talking,
//!   Interview, Offer, Closed), its match with why and gaps, a next step,
//!   notes, and "Not for me".
//! - **Today** (`GET /api/v1/today`): where the person is and their next
//!   step.
//!
//! What it needs from other modules (reading a posting from a link, how far
//! the profile is, the latest search) are [`ports::Neighbours`], which the
//! server implements; modules never import each other.
//!
//! Publishes `pipeline.opportunity_added` (ids only).

#![forbid(unsafe_code)]

pub mod events;
pub mod features;
pub mod ports;
pub mod store;

use std::sync::Arc;

use applicamp_kernel::{Module, jobs::Background};
use axum::Extension;
use utoipa_axum::{router::OpenApiRouter, routes};

pub use ports::Neighbours;

/// The module's name, schema and operation prefix.
pub const NAME: &str = "pipeline";

/// What the module is composed with.
#[derive(Clone, Default)]
pub struct PipelineDeps {
    /// The other modules, through the server. Without them (a catalogue
    /// composition) links are never read and Today is unavailable.
    pub neighbours: Option<Arc<dyn Neighbours>>,
}

impl std::fmt::Debug for PipelineDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineDeps")
            .field("neighbours", &self.neighbours.is_some())
            .finish()
    }
}

/// The pipeline module, composed with `deps`.
pub fn module(deps: PipelineDeps) -> Module {
    use features::{opportunities, today};
    let router = OpenApiRouter::new()
        .routes(routes!(opportunities::list, opportunities::add))
        .routes(routes!(opportunities::get, opportunities::update))
        .routes(routes!(today::get))
        .layer(Extension(deps));
    Module {
        name: NAME,
        router,
        background: Background::new().publishes::<events::OpportunityAdded>(),
    }
}

#[cfg(test)]
pub(crate) mod testkit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_is_named_after_its_schema() {
        assert_eq!(module(PipelineDeps::default()).name, "pipeline");
    }
}
