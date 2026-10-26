//! Cross-site request forgery protection.
//!
//! The session cookie is `SameSite=Strict`, which already keeps it off
//! cross-site requests. As a second, independent line, every unsafe request
//! (anything but `GET`, `HEAD`, `OPTIONS`, `TRACE`), on any path, must
//!
//! 1. carry `X-Applicamp-CSRF: 1`, a custom header a cross-site form or a
//!    "simple" cross-origin request cannot set without a CORS preflight the
//!    server never approves; and
//! 2. come from the app's own origin: an `Origin` equal to
//!    `APPLICAMP_PUBLIC_URL`'s, and no `Sec-Fetch-Site` other than
//!    `same-origin`.
//!
//! Anything else is refused with 403 problem+json (`csrf_check_failed`)
//! before any handler runs, so nothing changes.

use std::sync::Arc;

use applicamp_kernel::Problem;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse as _, Response},
};

/// The header the web client sends on every unsafe request.
pub const CSRF_HEADER: &str = "x-applicamp-csrf";

/// The problem `code` of a refused request.
pub const CSRF_FAILED: &str = "csrf_check_failed";

/// Middleware: refuse unsafe requests that fail the check. Its state is the
/// public origin (`https://applicamp.example.com`).
pub async fn check(State(origin): State<Arc<str>>, request: Request, next: Next) -> Response {
    if request.method().is_safe() || passes(request.headers(), &origin) {
        next.run(request).await
    } else {
        tracing::info!(method = %request.method(), "refused a request failing the CSRF check");
        Problem::new(
            StatusCode::FORBIDDEN,
            CSRF_FAILED,
            "This request did not come from Applicamp itself.",
        )
        .into_response()
    }
}

fn passes(headers: &HeaderMap, origin: &str) -> bool {
    let single = |name: &str| {
        let mut values = headers.get_all(name).iter();
        match (values.next(), values.next()) {
            (Some(value), None) => value.to_str().ok(),
            _ => None,
        }
    };
    if single(CSRF_HEADER) != Some("1") || single(header::ORIGIN.as_str()) != Some(origin) {
        return false;
    }
    // Browsers that send fetch metadata say where the request came from.
    headers.get("sec-fetch-site").is_none() || single("sec-fetch-site") == Some("same-origin")
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    const ORIGIN: &str = "https://applicamp.example.com";

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn same_origin_requests_with_the_header_pass() {
        assert!(passes(
            &headers(&[(CSRF_HEADER, "1"), ("origin", ORIGIN)]),
            ORIGIN
        ));
        assert!(passes(
            &headers(&[
                (CSRF_HEADER, "1"),
                ("origin", ORIGIN),
                ("sec-fetch-site", "same-origin")
            ]),
            ORIGIN
        ));
    }

    #[test]
    fn everything_else_fails() {
        for case in [
            headers(&[("origin", ORIGIN)]),
            headers(&[(CSRF_HEADER, "0"), ("origin", ORIGIN)]),
            headers(&[(CSRF_HEADER, "1"), (CSRF_HEADER, "1"), ("origin", ORIGIN)]),
            headers(&[(CSRF_HEADER, "1")]),
            headers(&[(CSRF_HEADER, "1"), ("sec-fetch-site", "same-origin")]),
            headers(&[(CSRF_HEADER, "1"), ("origin", "https://evil.example.com")]),
            headers(&[(CSRF_HEADER, "1"), ("origin", "null")]),
            headers(&[
                (CSRF_HEADER, "1"),
                ("origin", ORIGIN),
                ("sec-fetch-site", "cross-site"),
            ]),
            headers(&[
                (CSRF_HEADER, "1"),
                ("origin", ORIGIN),
                ("sec-fetch-site", "same-site"),
            ]),
        ] {
            assert!(!passes(&case, ORIGIN), "{case:?}");
        }
    }
}
