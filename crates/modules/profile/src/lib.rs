//! `applicamp-profile`: what Applicamp knows about a person's work.
//!
//! - **The master profile** ([`schema::Profile`]): headline, summary,
//!   positions, education, skills, languages, certifications, projects and
//!   links, in JSON Resume's names (ADR 0007). Agents use only what it says.
//! - **Imports**: a LinkedIn data export ZIP (read from its CSVs, no model),
//!   a PDF or pasted text (structured by the `complete` model, then the
//!   source check) becomes a draft the person reviews, then applies
//!   (Replace or Merge) or discards. Files are never stored.
//!
//! - **Goals** ([`goals::Goals`]): what they want next, answered one
//!   question at a time and versioned.
//!
//! - **Grill me's proposals** ([`features::grill`]): the facts taken from an
//!   answer about a job wait on the position to be kept, fixed or dropped.
//!   Its interview was retired from the client (#44) and removed (#49).
//!
//! - **Tell me about you** ([`features::about`]): what Applicamp knows and
//!   assumes about a person as one portrait, confirmed by them, then the
//!   questions about the gaps (decided in code, worded by the model), then
//!   saved in one write.
//!
//! - **Matching** ([`matching`]): how far a person is, and what a job search
//!   matches against, for the ports the server wires to other modules.
//!
//! - **Completion** ([`features::completion`]): how strong the profile is
//!   ([`strength`]) and the order of the skills. Its AI questions, drafts and
//!   found skills were removed (#49): Tell me about you asks about the gaps.
//!
//! Publishes `profile.updated` (ids only) whenever a profile changes and
//! `profile.goals_updated` (id and version) whenever the goals do.

#![forbid(unsafe_code)]

pub mod events;
pub mod features;
pub mod goals;
pub mod matching;
pub mod ports;
pub mod schema;
pub mod strength;

use applicamp_ai::Gateway;
use applicamp_kernel::{Module, jobs::Background};
use axum::{Extension, extract::DefaultBodyLimit};
use utoipa_axum::{router::OpenApiRouter, routes};

pub use features::imports::{Ai, ImportKind};

/// The module's name, schema and operation prefix.
pub const NAME: &str = "profile";

/// What the module is composed with.
#[derive(Debug, Clone, Default)]
pub struct ProfileDeps {
    /// The AI gateway; reading a PDF or pasted text needs its `complete`
    /// role. Without it only the LinkedIn export can be imported.
    pub ai: Option<Gateway>,
    /// Reads a past employer's own website for "Tell me about you". Without
    /// it no employer is researched; the portrait is made from the profile
    /// and what is usual for the roles.
    pub research: ports::Researcher,
}

/// The profile module, composed with `deps`.
pub fn module(deps: ProfileDeps) -> Module {
    use features::{about, completion, goals, grill, imports, profile};
    // Uploads take up to a 20 MB export; every other route keeps axum's
    // default body limit.
    let uploads = OpenApiRouter::new()
        .routes(routes!(imports::create))
        .layer(DefaultBodyLimit::max(imports::UPLOAD_BODY_LIMIT));
    let router = OpenApiRouter::new()
        .routes(routes!(profile::get, profile::update))
        .routes(routes!(goals::get, goals::update))
        .routes(routes!(completion::get_strength))
        .routes(routes!(completion::skills::order_skills))
        .routes(routes!(grill::get))
        .routes(routes!(grill::keep_all))
        .routes(routes!(grill::keep_proposal))
        .routes(routes!(grill::drop_proposal))
        .routes(routes!(about::get))
        .routes(routes!(about::portrait))
        .routes(routes!(about::choose))
        .routes(routes!(about::accuracy))
        .routes(routes!(about::employer_site))
        .routes(routes!(about::employer_retry))
        .routes(routes!(about::extra))
        .routes(routes!(about::change, about::remove_change))
        .routes(routes!(about::review))
        .routes(routes!(about::save))
        .routes(routes!(about::questions::next))
        .routes(routes!(about::questions::enough))
        .routes(routes!(about::questions::answer))
        .routes(routes!(about::questions::skip))
        .routes(routes!(imports::get))
        .routes(routes!(imports::apply))
        .routes(routes!(imports::discard))
        .merge(uploads)
        .layer(Extension(Ai(deps.ai.clone())))
        .layer(Extension(deps.research.clone()));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job::<imports::ExpireImports>()
            .publishes::<events::ProfileUpdated>()
            .publishes::<events::GoalsUpdated>(),
        kinds: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod testkit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_is_named_after_its_schema() {
        assert_eq!(module(ProfileDeps::default()).name, "profile");
    }

    #[test]
    fn its_background_work_registers() {
        let registry = applicamp_kernel::jobs::Registry::build(vec![(
            NAME,
            module(ProfileDeps::default()).background,
        )])
        .unwrap();
        assert_eq!(registry.kinds(), ["profile.expire_imports"]);
    }
}
