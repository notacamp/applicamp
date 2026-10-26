//! The web client: the build of `clients/app`, served from
//! `APPLICAMP_WEB_DIR` beside the API.
//!
//! A path that names a file in the build gets that file; any other `GET`
//! gets `index.html`, so the client's own routes (`/pipeline`, `/sign-in`)
//! load the app, which routes in the browser. The API (`/api/…`), the BFF
//! (`/bff/…`) and `/healthz` never fall through to the client: an unknown
//! path there is the API's problem+json 404, as is any other method.
//!
//! Vite names everything under `/assets/` after its content, so those
//! files are cached for a year; `index.html` and the other files are
//! revalidated on every load, so a new release is picked up at once. A
//! missing asset is a 404, never `index.html`.

use std::path::Path;

use applicamp_kernel::{Error, config::ConfigError};
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse as _, Response},
};
use tower::ServiceExt as _;
use tower_http::services::{ServeDir, ServeFile};

/// The build's content-named files.
const ASSETS: &str = "/assets/";
/// Path prefixes the server answers itself.
const SERVER_PREFIXES: [&str; 2] = ["/api", "/bff"];
/// `Cache-Control` of a content-named asset.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// `Cache-Control` of everything else in the build.
const REVALIDATE: &str = "no-cache";

/// The web client's files.
#[derive(Debug, Clone)]
pub struct Web {
    /// `/assets/…`: a file, or nothing.
    assets: ServeDir,
    /// Everything else: a file, or `index.html`.
    pages: ServeDir<ServeFile>,
}

impl Web {
    /// The build in `dir`, which must hold an `index.html`.
    pub fn new(dir: &Path) -> Result<Self, ConfigError> {
        let index = dir.join("index.html");
        if !index.is_file() {
            return Err(ConfigError(format!(
                "APPLICAMP_WEB_DIR={}: no index.html there (the build of clients/app)",
                dir.display()
            )));
        }
        Ok(Self {
            assets: ServeDir::new(dir).append_index_html_on_directories(false),
            pages: ServeDir::new(dir)
                .append_index_html_on_directories(false)
                .fallback(ServeFile::new(index)),
        })
    }

    /// The answer to `request`, which no API or BFF route matched.
    pub async fn serve(self, request: Request) -> Response {
        let path = request.uri().path();
        if is_server_path(path) || !matches!(*request.method(), Method::GET | Method::HEAD) {
            return Error::NotFound.into_response();
        }
        let asset = path.starts_with(ASSETS);
        let Ok(response) = if asset {
            self.assets.oneshot(request).await
        } else {
            self.pages.oneshot(request).await
        };
        if response.status() == StatusCode::NOT_FOUND {
            return Error::NotFound.into_response();
        }
        let (mut parts, body) = response.into_parts();
        let cache = if asset && parts.status.is_success() {
            IMMUTABLE
        } else {
            REVALIDATE
        };
        parts
            .headers
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
        Response::from_parts(parts, Body::new(body))
    }
}

/// Whether the server itself owns `path` (so it never serves the client).
fn is_server_path(path: &str) -> bool {
    path == crate::HEALTH_PATH
        || SERVER_PREFIXES.iter().any(|prefix| {
            path.strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use http_body_util::BodyExt as _;

    use super::*;

    const INDEX: &str = "<!doctype html><title>Applicamp</title>";

    /// A build directory with `index.html`, one asset and one public file,
    /// next to a file outside it that must never be served.
    fn build(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("applicamp-web-{name}-{}", std::process::id()));
        let dir = root.join("dist");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), INDEX).unwrap();
        std::fs::write(dir.join("assets/app-1a2b3c.js"), "console.log(1)").unwrap();
        std::fs::write(dir.join("theme-boot.js"), "void 0").unwrap();
        std::fs::write(root.join("outside.txt"), "not for the web").unwrap();
        dir
    }

    async fn get(web: &Web, method: Method, uri: &str) -> (StatusCode, String, String, String) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let response = web.clone().serve(request).await;
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned()
        };
        let (content_type, cache) = (header(header::CONTENT_TYPE), header(header::CACHE_CONTROL));
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            content_type,
            cache,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    #[tokio::test]
    async fn client_routes_load_the_app() {
        let web = Web::new(&build("routes")).unwrap();
        for path in [
            "/",
            "/pipeline",
            "/sign-in?error=sign_in_failed",
            "/not-invited",
        ] {
            let (status, content_type, cache, body) = get(&web, Method::GET, path).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            assert!(
                content_type.starts_with("text/html"),
                "{path}: {content_type}"
            );
            assert_eq!(cache, REVALIDATE, "{path}");
            assert_eq!(body, INDEX, "{path}");
        }
    }

    #[tokio::test]
    async fn files_are_served_and_assets_cached_for_good() {
        let web = Web::new(&build("files")).unwrap();
        let (status, content_type, cache, body) =
            get(&web, Method::GET, "/assets/app-1a2b3c.js").await;
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.contains("javascript"), "{content_type}");
        assert_eq!(cache, IMMUTABLE);
        assert_eq!(body, "console.log(1)");

        let (status, _, cache, body) = get(&web, Method::GET, "/theme-boot.js").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cache, REVALIDATE);
        assert_eq!(body, "void 0");
    }

    #[tokio::test]
    async fn the_server_paths_and_missing_assets_stay_404() {
        let web = Web::new(&build("server-paths")).unwrap();
        for path in [
            "/api",
            "/api/v1/nothing",
            "/bff/nothing",
            "/healthz",
            "/assets/missing.js",
        ] {
            let (status, content_type, _, _) = get(&web, Method::GET, path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(content_type, "application/problem+json", "{path}");
        }
        let (status, _, _, _) = get(&web, Method::POST, "/pipeline").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        // Paths that only start like a server prefix are the client's.
        let (status, _, _, body) = get(&web, Method::GET, "/apis").await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, INDEX));
    }

    #[tokio::test]
    async fn nothing_outside_the_build_is_served() {
        let web = Web::new(&build("outside")).unwrap();
        for path in [
            "/../outside.txt",
            "/%2e%2e/outside.txt",
            "/assets/../../outside.txt",
        ] {
            let (_, _, _, body) = get(&web, Method::GET, path).await;
            assert!(!body.contains("not for the web"), "{path}");
        }
    }

    #[test]
    fn a_directory_without_a_build_is_refused() {
        let dir = std::env::temp_dir().join(format!("applicamp-web-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = Web::new(&dir).unwrap_err().to_string();
        assert!(err.contains("APPLICAMP_WEB_DIR"), "{err}");
    }

    #[test]
    fn server_paths() {
        for path in [
            "/api",
            "/api/",
            "/api/v1/me",
            "/bff",
            "/bff/providers",
            "/healthz",
        ] {
            assert!(is_server_path(path), "{path}");
        }
        for path in [
            "/",
            "/apis",
            "/bffs",
            "/pipeline",
            "/assets/x.js",
            "/healthz/x",
        ] {
            assert!(!is_server_path(path), "{path}");
        }
    }
}
