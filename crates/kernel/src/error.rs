//! The one error type handlers return, rendered as RFC 9457
//! `application/problem+json`.
//!
//! Every problem carries a stable `code` clients match on (and translate);
//! `title` and `detail` are English. Permission failures on *reads* use
//! [`Error::NotFound`], not [`Error::Forbidden`], so a caller cannot probe
//! for records it may not see.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Media type of problem details (RFC 9457 §3).
pub const PROBLEM_JSON: &str = "application/problem+json";

/// Errors every module can return from a handler.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The resource does not exist, or the caller may not see it.
    #[error("not found")]
    NotFound,
    /// The request is malformed or invalid: a code and what is wrong.
    #[error("bad request: {0}")]
    BadRequest(&'static str, String),
    /// No live session (or other credential): sign in first.
    #[error("unauthorized: {0}")]
    Unauthorized(&'static str),
    /// This *action* is refused (never used for reads).
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    /// The request conflicts with the current state.
    #[error("conflict: {0}")]
    Conflict(&'static str, String),
    /// A dependency (database, provider) is unavailable.
    #[error("unavailable: {0}")]
    Unavailable(&'static str),
    /// Anything else. Logged; the client sees a generic message.
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Kernel result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Wrap any error as [`Error::Internal`].
    pub fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Internal(err.into())
    }

    /// HTTP status this error maps to.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(..) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(..) => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The stable code clients match on.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::BadRequest(code, _)
            | Self::Conflict(code, _)
            | Self::Unauthorized(code)
            | Self::Forbidden(code)
            | Self::Unavailable(code) => *code,
            Self::Internal(_) => "internal_error",
        }
    }

    /// The problem document. Never includes internal detail.
    pub fn to_problem(&self) -> Problem {
        let detail = match self {
            Self::NotFound => "This resource does not exist, or you cannot see it.".to_owned(),
            Self::BadRequest(_, detail) | Self::Conflict(_, detail) => detail.clone(),
            Self::Unauthorized(_) => "Sign in first.".to_owned(),
            Self::Forbidden(_) => "You may not do this.".to_owned(),
            Self::Unavailable(_) => {
                "Something this needs is unavailable. Try again shortly.".to_owned()
            }
            Self::Internal(_) => "Something went wrong on our side.".to_owned(),
        };
        Problem::new(self.status(), self.code(), detail)
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::PoolTimedOut => Self::Unavailable("database_unavailable"),
            other => Self::internal(other),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if let Self::Internal(source) = &self {
            // The Display form only: a database error's message names the
            // constraint, never the row's values.
            tracing::error!(error = %source, "internal error");
        }
        self.to_problem().into_response()
    }
}

/// RFC 9457 problem details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "type": "about:blank",
    "title": "Not Found",
    "status": 404,
    "detail": "This resource does not exist, or you cannot see it.",
    "code": "not_found"
}))]
pub struct Problem {
    /// URI identifying the problem type; `about:blank` when the HTTP status
    /// says it all.
    #[serde(rename = "type")]
    pub kind: String,
    /// Short summary: the HTTP status's reason phrase.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// What went wrong in this occurrence, in English.
    pub detail: String,
    /// Stable machine-readable reason (`not_found`, `session_required`, …).
    /// Match and translate on this, never on `title` or `detail`.
    pub code: String,
}

impl Problem {
    /// A problem with `status`, `code` and `detail`.
    pub fn new(status: StatusCode, code: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: "about:blank".to_owned(),
            title: status.canonical_reason().unwrap_or("Error").to_owned(),
            status: status.as_u16(),
            detail: detail.into(),
            code: code.to_owned(),
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        response
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt as _;

    use super::*;

    async fn render(err: Error) -> (StatusCode, String, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let content_type = response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .to_owned();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, content_type, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn not_found_is_problem_json() {
        let (status, content_type, body) = render(Error::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(content_type, PROBLEM_JSON);
        assert_eq!(
            body,
            serde_json::json!({
                "type": "about:blank",
                "title": "Not Found",
                "status": 404,
                "detail": "This resource does not exist, or you cannot see it.",
                "code": "not_found"
            })
        );
    }

    #[tokio::test]
    async fn internal_hides_detail() {
        let (status, _, body) = render(Error::internal("password=hunter2 leaked")).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "internal_error");
        assert!(!body.to_string().contains("hunter2"));
    }

    #[tokio::test]
    async fn bad_request_carries_its_code_and_detail() {
        let (status, _, body) =
            render(Error::BadRequest("invalid_email", "Not an address.".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "invalid_email");
        assert_eq!(body["detail"], "Not an address.");
    }

    #[test]
    fn database_errors_map() {
        assert!(matches!(
            Error::from(sqlx::Error::RowNotFound),
            Error::NotFound
        ));
        let busy = Error::from(sqlx::Error::PoolTimedOut);
        assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(busy.code(), "database_unavailable");
    }
}
