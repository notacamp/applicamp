//! `applicamp-kernel`: the domain-agnostic core every crate builds on.
//!
//! It depends on no other Applicamp crate and knows nothing about jobs to
//! hunt, CVs or approvals (enforced by `cargo xtask arch`). A trimmed copy of
//! Campbooks Next's kernel: no workspaces, no row-level security, no token
//! issuer.

#![forbid(unsafe_code)]

pub mod auth;
pub mod claims;
pub mod clock;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod events;
pub mod ids;
pub mod jobs;
pub mod module;
pub mod state;
pub mod tech;
pub mod telemetry;

pub use config::Config;
pub use error::{Error, Problem, Result};
pub use events::{DomainEvent, Event, EventBus, EventHandler};
pub use jobs::{Background, Job, JobContext, JobError};
pub use module::{BoxFuture, Module};
pub use state::AppState;
