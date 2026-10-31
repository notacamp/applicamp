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
//! - **The brief** (`/api/v1/opportunities/{id}/brief`): what a tailored CV
//!   is based on. The company's own pages (its site, asked once; read on a
//!   click, cached 30 days) become facts that each quote their page; the
//!   posting becomes what the job wants most and up to three questions where
//!   the profile is thin; an answer becomes facts on a position, kept with
//!   Grill me's Keep, Fix or Drop.
//!
//! - **The tailored CV** (`/api/v1/opportunities/{id}/cv`): a draft per
//!   opportunity (one model call), every line citing the profile sources it
//!   comes from, a fact check without a model that blocks Approve while a
//!   line says what its sources do not, review (edit, remove, "it's true",
//!   put back) and approval as a version.
//!
//! - **The tailored CV as a PDF** (#38): approving renders the approved text
//!   with Typst inside the server (Clean or Compact, 1 or 2 pages), reads the
//!   PDF's text back and compares it with the CV, and keeps the PDF as a
//!   version (`/api/v1/opportunities/{id}/cv/versions/{n}/pdf`); a new template
//!   or length is a new version, with no AI call; `/api/v1/cvs` lists them all.
//!
//! Publishes `pipeline.opportunity_added` (ids only).

#![forbid(unsafe_code)]

pub mod companies;
pub mod events;
pub mod features;
pub mod ports;
pub mod store;

use std::sync::Arc;

use applicamp_ai::Gateway;
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
    /// The AI gateway: its `complete` role reads a company's pages and
    /// compares a job with the profile. Without it those say they are
    /// unavailable.
    pub ai: Option<Gateway>,
}

impl std::fmt::Debug for PipelineDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineDeps")
            .field("neighbours", &self.neighbours.is_some())
            .field("ai", &self.ai.is_some())
            .finish()
    }
}

/// The pipeline module, composed with `deps`.
pub fn module(deps: PipelineDeps) -> Module {
    use features::{brief, cv, opportunities, today};
    let router = OpenApiRouter::new()
        .routes(routes!(opportunities::list, opportunities::add))
        .routes(routes!(opportunities::get, opportunities::update))
        .routes(routes!(brief::get))
        .routes(routes!(brief::set_site))
        .routes(routes!(brief::read))
        .routes(routes!(brief::remove_fact))
        .routes(routes!(brief::restore_fact))
        .routes(routes!(brief::answer))
        .routes(routes!(brief::dont_have))
        .routes(routes!(brief::skip))
        .routes(routes!(cv::get))
        .routes(routes!(cv::create))
        .routes(routes!(cv::change_line))
        .routes(routes!(cv::confirm_line))
        .routes(routes!(cv::put_back))
        .routes(routes!(cv::approve))
        .routes(routes!(cv::versions::create))
        .routes(routes!(cv::versions::pdf_file))
        .routes(routes!(cv::versions::list_all))
        .routes(routes!(cv::discard))
        .routes(routes!(today::get))
        .layer(Extension(deps));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job::<cv::ExpireCvDrafts>()
            .publishes::<events::OpportunityAdded>(),
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
