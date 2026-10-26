//! `applicamp-identity`: who may sign in, and who they are.
//!
//! - **Accounts**: one per person, created on a first sign-in whose verified
//!   address is on the invite list.
//! - **Ways in**: the provider identities (Campbooks, LinkedIn) that sign an
//!   account in; one account can have both.
//! - **Invites**: the list of addresses that may create an account, managed
//!   from the server's command line only.
//!
//! The server's backend-for-frontend runs the OpenID Connect round trip and
//! hands the validated identity to [`sign_in`]; this module decides.

#![forbid(unsafe_code)]

pub mod events;
pub mod features;
mod provider;

use applicamp_kernel::{Module, jobs::Background};
use utoipa_axum::{router::OpenApiRouter, routes};

pub use features::{
    invites,
    sign_in::{ProviderIdentity, SignIn, sign_in},
};
pub use provider::Provider;

/// The module's name, schema and operation prefix.
pub const NAME: &str = "identity";

/// The identity module.
pub fn module() -> Module {
    Module {
        name: NAME,
        router: OpenApiRouter::new().routes(routes!(features::me::handler)),
        background: Background::new().publishes::<events::AccountCreated>(),
    }
}
