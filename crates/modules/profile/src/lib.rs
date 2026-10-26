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
//! Publishes `profile.updated` (ids only) whenever a profile changes.

#![forbid(unsafe_code)]

pub mod events;
pub mod features;
pub mod schema;

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
}

/// The profile module, composed with `deps`.
pub fn module(deps: ProfileDeps) -> Module {
    use features::{imports, profile};
    // Uploads take up to a 20 MB export; every other route keeps axum's
    // default body limit.
    let uploads = OpenApiRouter::new()
        .routes(routes!(imports::create))
        .layer(DefaultBodyLimit::max(imports::UPLOAD_BODY_LIMIT));
    let router = OpenApiRouter::new()
        .routes(routes!(profile::get, profile::update))
        .routes(routes!(imports::get))
        .routes(routes!(imports::apply))
        .routes(routes!(imports::discard))
        .merge(uploads)
        .layer(Extension(Ai(deps.ai)));
    Module {
        name: NAME,
        router,
        background: Background::new()
            .job::<imports::ExpireImports>()
            .publishes::<events::ProfileUpdated>(),
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
