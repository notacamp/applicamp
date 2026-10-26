//! The contract between a domain module and the server.
//!
//! A module crate exposes `pub fn module() -> Module`; `applicamp-server`
//! collects them, merges their routers and registers their background work.
//! Routes are declared with `utoipa-axum` (`routes!`) so every endpoint lands
//! in the OpenAPI document by construction.

use utoipa_axum::router::OpenApiRouter;

use crate::{jobs::Background, state::AppState};

/// A domain module, as handed to the server.
pub struct Module {
    /// The module's name; equals its crate directory and Postgres schema.
    pub name: &'static str,
    /// The module's routes (under `/api/v1/…`) with their OpenAPI docs.
    pub router: OpenApiRouter<AppState>,
    /// The module's jobs and event handlers, each named `<name>.<…>`.
    pub background: Background,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name)
            .field("background", &self.background)
            .finish_non_exhaustive()
    }
}
