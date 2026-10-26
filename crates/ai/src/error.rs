use crate::Role;

/// Problem code of a feature whose model is not configured.
pub const AI_UNAVAILABLE: &str = "ai_unavailable";
/// Problem code of an account over its daily token cap.
pub const AI_DAILY_LIMIT_REACHED: &str = "ai_daily_limit_reached";
/// Problem code of a model call that failed (the provider was down, or its
/// answer was unusable).
pub const AI_FAILED: &str = "ai_failed";

/// Errors from the AI gateway. None carries a prompt or an answer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No provider is configured for this role.
    #[error(
        "AI role `{0}` is not configured (set APPLICAMP_AI__{upper}__BASE_URL and __MODEL)",
        upper = .0.as_str().to_uppercase()
    )]
    NotConfigured(Role),
    /// The AI configuration is invalid.
    #[error("invalid AI configuration: {0}")]
    Config(String),
    /// The provider could not be reached, or timed out.
    #[error("AI provider request failed: {0}")]
    Transport(#[source] reqwest::Error),
    /// The provider answered with a non-success status. Its body is not
    /// kept: a provider's error can echo the request.
    #[error("AI provider returned {status}")]
    Status {
        /// HTTP status code.
        status: u16,
    },
    /// The answer did not have the expected shape. Says what was wrong,
    /// never the answer's text.
    #[error("unexpected AI provider response: {0}")]
    InvalidResponse(String),
    /// The account used its tokens for today; nothing was sent.
    #[error("the account reached its daily AI limit of {cap} tokens; nothing was sent")]
    OverDailyCap {
        /// Tokens an account may use per UTC day.
        cap: u64,
    },
    /// The call could not be counted (the ledger is unavailable), so it was
    /// not made: a broken ledger never lifts the cap.
    #[error("the AI call could not be counted; nothing was sent")]
    Metering,
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        // Without the URL: it can carry a query string.
        Self::Transport(err.without_url())
    }
}

impl From<Error> for applicamp_kernel::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::NotConfigured(_) => Self::Unavailable(AI_UNAVAILABLE),
            Error::OverDailyCap { .. } => Self::Unavailable(AI_DAILY_LIMIT_REACHED),
            Error::Transport(_)
            | Error::Status { .. }
            | Error::InvalidResponse(_)
            | Error::Metering => {
                tracing::warn!(error = %err, "an AI call failed");
                Self::Unavailable(AI_FAILED)
            }
            Error::Config(_) => Self::internal(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_become_stable_problem_codes() {
        let unavailable = applicamp_kernel::Error::from(Error::NotConfigured(Role::Complete));
        assert_eq!(unavailable.code(), AI_UNAVAILABLE);
        assert_eq!(unavailable.status().as_u16(), 503);
        let capped = applicamp_kernel::Error::from(Error::OverDailyCap { cap: 10 });
        assert_eq!(capped.code(), AI_DAILY_LIMIT_REACHED);
        let failed = applicamp_kernel::Error::from(Error::Status { status: 502 });
        assert_eq!(failed.code(), AI_FAILED);
    }

    #[test]
    fn a_status_error_shows_no_body() {
        let err = Error::Status { status: 400 };
        assert_eq!(err.to_string(), "AI provider returned 400");
    }
}
