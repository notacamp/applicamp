//! The contract between a domain module and the server.
//!
//! A module crate exposes `pub fn module() -> Module`; `applicamp-server`
//! collects them, merges their routers and registers their background work.
//! Routes are declared with `utoipa-axum` (`routes!`) so every endpoint lands
//! in the OpenAPI document by construction.

use std::{future::Future, pin::Pin};

use utoipa_axum::router::OpenApiRouter;

use crate::{agent::RunKind, jobs::Background, state::AppState};

/// A domain module, as handed to the server.
pub struct Module {
    /// The module's name; equals its crate directory and Postgres schema.
    pub name: &'static str,
    /// The module's routes (under `/api/v1/…`) with their OpenAPI docs.
    pub router: OpenApiRouter<AppState>,
    /// The module's jobs and event handlers, each named `<name>.<…>`.
    pub background: Background,
    /// The kinds of agent run it owns (`<name>.<kind>`), which the `runs`
    /// module executes.
    pub kinds: Vec<RunKind>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name)
            .field("background", &self.background)
            .field(
                "kinds",
                &self.kinds.iter().map(|kind| kind.name).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

/// The future a **port** returns: a trait a module declares for what it
/// needs from another module, implemented by the server (the only crate
/// that composes modules). Modules never import each other; a port keeps
/// it so while letting one ask another a question synchronously.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
