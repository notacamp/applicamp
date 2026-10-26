//! Who is making a request.
//!
//! The server's backend-for-frontend resolves the session cookie before a
//! module's handler runs and leaves a [`Principal`] in the request's
//! extensions; a handler that needs a signed-in person takes [`SignedIn`],
//! which answers 401 `session_required` when there is none. Modules never
//! read cookies or the `bff` schema themselves.

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::Error;

/// The problem code of a request that needs a session and has none.
pub const SESSION_REQUIRED: &str = "session_required";

/// The signed-in person behind a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Principal {
    /// Their account (`identity.accounts.id`).
    pub account_id: Uuid,
    /// The session the request came with (`bff.sessions.id`).
    pub session_id: Uuid,
}

/// Extractor: the request's [`Principal`], or 401 `session_required`.
#[derive(Debug, Clone, Copy)]
pub struct SignedIn(pub Principal);

impl<S: Send + Sync> FromRequestParts<S> for SignedIn {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Principal>()
            .copied()
            .map(Self)
            .ok_or(Error::Unauthorized(SESSION_REQUIRED))
    }
}
