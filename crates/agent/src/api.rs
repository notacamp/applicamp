//! The agent API, as the runner calls it: its run's token as a bearer,
//! every event both on standard output (one JSON line) and through the API,
//! numbered so the server keeps each once.

use std::{
    io::Write as _,
    sync::atomic::{AtomicI64, Ordering},
    time::Duration,
};

use reqwest::{Client, NoProxy, Proxy, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use url::Url;

/// How long one call may take.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Times the context is asked again while the run is not found.
const CONTEXT_RETRIES: u32 = 5;
/// The wait before asking again.
const CONTEXT_RETRY_AFTER: Duration = Duration::from_millis(500);

/// A call failed.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The server refused it: its status and problem code.
    #[error("refused ({status}): {code}")]
    Refused {
        /// The HTTP status.
        status: StatusCode,
        /// The problem's `code`.
        code: String,
    },
    /// The server could not be reached, or answered nonsense.
    #[error("unreachable: {0}")]
    Unreachable(String),
}

impl ApiError {
    /// The problem code of a refusal.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Refused { code, .. } => Some(code),
            Self::Unreachable(_) => None,
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        // Without the URL: nothing secret is in it, but nothing is needed.
        Self::Unreachable(err.without_url().to_string())
    }
}

/// An event line.
#[derive(Debug, Clone, Serialize)]
pub struct Line<'a> {
    pub seq: i64,
    pub level: &'a str,
    pub message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<&'a str>,
}

/// The HTTP client of the sandbox: through the proxy its environment names
/// (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`), except the hosts in
/// `NO_PROXY`.
pub fn client() -> Result<Client, ApiError> {
    let var = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
    };
    let mut builder = Client::builder().timeout(TIMEOUT).no_proxy();
    let all = var(&["ALL_PROXY", "all_proxy"]);
    if let Some(proxy) = var(&["HTTPS_PROXY", "https_proxy"]).or_else(|| all.clone()) {
        builder = builder.proxy(Proxy::https(proxy.as_str())?.no_proxy(NoProxy::from_env()));
    }
    if let Some(proxy) = var(&["HTTP_PROXY", "http_proxy"]).or(all) {
        builder = builder.proxy(Proxy::http(proxy.as_str())?.no_proxy(NoProxy::from_env()));
    }
    Ok(builder.build()?)
}

/// One run's view of the agent API.
pub struct Api {
    http: Client,
    base: Url,
    token: String,
    seq: AtomicI64,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

impl Api {
    /// The agent API at `base`, called with `token` over `http`.
    pub fn new(http: Client, base: Url, token: String) -> Self {
        Self {
            http,
            base,
            token,
            seq: AtomicI64::new(0),
        }
    }

    /// The HTTP client (for the run's own requests).
    pub fn http(&self) -> &Client {
        &self.http
    }

    fn url(&self, path: &str) -> Result<Url, ApiError> {
        self.base
            .join(&format!("api/agent/v1/{path}"))
            .map_err(|e| ApiError::Unreachable(format!("bad agent API address: {e}")))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, ApiError> {
        let response = request.bearer_auth(&self.token).send().await?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            Ok(body)
        } else {
            Err(ApiError::Refused {
                status,
                code: body["code"].as_str().unwrap_or("unknown").to_owned(),
            })
        }
    }

    /// `GET context`, asked again a few times while the run is not found
    /// yet (the server may not have recorded its sandbox when the runner
    /// starts).
    pub async fn context(&self) -> Result<Value, ApiError> {
        let mut attempt = 0;
        loop {
            match self.send(self.http.get(self.url("context")?)).await {
                Err(ApiError::Refused { status, .. })
                    if status == StatusCode::NOT_FOUND && attempt < CONTEXT_RETRIES =>
                {
                    attempt += 1;
                    tokio::time::sleep(CONTEXT_RETRY_AFTER).await;
                }
                other => return other,
            }
        }
    }

    /// `POST tools/{name}`: the tool's result.
    pub async fn tool(&self, name: &str, args: Value) -> Result<Value, ApiError> {
        let body = self
            .send(
                self.http
                    .post(self.url(&format!("tools/{name}"))?)
                    .json(&json!({ "args": args })),
            )
            .await?;
        Ok(body["result"].clone())
    }

    /// `POST finish`.
    pub async fn finish(&self, outcome: &str, summary: Value) -> Result<(), ApiError> {
        self.send(
            self.http
                .post(self.url("finish")?)
                .json(&json!({ "outcome": outcome, "summary": summary })),
        )
        .await
        .map(|_| ())
    }

    /// Write an event: on standard output, then through the API (best
    /// effort: the output line reaches the server too).
    pub async fn event(
        &self,
        level: &str,
        message: &str,
        operation: Option<&str>,
        tool_name: Option<&str>,
    ) {
        let line = Line {
            seq: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
            level,
            message,
            operation,
            tool_name,
        };
        if let Ok(json) = serde_json::to_string(&line) {
            let _ = writeln!(std::io::stdout(), "{json}");
        }
        if let Ok(url) = self.url("events") {
            let _ = self.send(self.http.post(url).json(&[&line])).await;
        }
    }
}
