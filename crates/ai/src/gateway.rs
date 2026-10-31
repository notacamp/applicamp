//! The gateway: one client for every role, over the OpenAI-compatible chat
//! completions API.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, StatusCode, header::RETRY_AFTER};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    AiConfig, Error, ProviderConfig, Role,
    config::bearer,
    price::{self, Price},
    usage::{self, Limit, Limits, Reservation},
};

/// Default TCP connect timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default whole-request timeout: structuring a long CV takes a while.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How a call retries a provider that is busy (429) or failing (5xx), or
/// could not be reached: wait `base_delay`, doubling up to `max_delay`; a
/// `Retry-After` in seconds wins when it asks for longer (never past
/// `max_delay`). Timeouts are not retried: a retry would only double the
/// wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts in total, the first included (1 disables retries).
    pub attempts: u32,
    /// Wait before the first retry.
    pub base_delay: Duration,
    /// Longest wait between attempts.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        }
    }
}

impl RetryPolicy {
    fn delay(&self, retry: u32, retry_after: Option<Duration>) -> Duration {
        let backoff = self
            .base_delay
            .saturating_mul(2u32.saturating_pow(retry))
            .min(self.max_delay);
        retry_after.map_or(backoff, |after| after.min(self.max_delay).max(backoff))
    }
}

/// Who said a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Instructions.
    System,
    /// The input.
    User,
    /// A previous model reply.
    Assistant,
}

/// A chat message.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Author.
    pub role: MessageRole,
    /// The text.
    pub content: String,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The length only: messages carry people's CVs.
        f.debug_struct("Message")
            .field("role", &self.role)
            .field("content_len", &self.content.len())
            .finish()
    }
}

impl Message {
    /// A system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: text.into(),
        }
    }

    /// A user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: text.into(),
        }
    }
}

/// The shape the answer must take.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Any JSON object.
    JsonObject,
    /// JSON matching a schema.
    JsonSchema {
        /// The schema, named.
        json_schema: JsonSchema,
    },
}

/// A named JSON schema, as `response_format` carries it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonSchema {
    /// A name for the schema (`profile`).
    pub name: String,
    /// The JSON schema.
    pub schema: serde_json::Value,
    /// Strict mode needs every property required, which optional fields are
    /// not; the answer is parsed and checked by the caller instead.
    pub strict: bool,
}

impl ResponseFormat {
    /// JSON matching `schema`, named `name`.
    pub fn json_schema(name: impl Into<String>, schema: serde_json::Value) -> Self {
        Self::JsonSchema {
            json_schema: JsonSchema {
                name: name.into(),
                schema,
                strict: false,
            },
        }
    }
}

/// A chat completion request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatRequest {
    /// The conversation, instructions first.
    pub messages: Vec<Message>,
    /// Sampling temperature; `None` is the provider's default.
    pub temperature: Option<f32>,
    /// Longest answer, in tokens. Reasoning models think before they answer:
    /// leave them room.
    pub max_tokens: Option<u32>,
    /// The shape of the answer.
    pub response_format: Option<ResponseFormat>,
}

impl ChatRequest {
    /// A request of `messages`.
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            ..Self::default()
        }
    }
}

/// Tokens a call used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Usage {
    /// Prompt tokens.
    #[serde(default)]
    pub prompt_tokens: u32,
    /// Completion tokens.
    #[serde(default)]
    pub completion_tokens: u32,
}

/// A chat completion.
#[derive(Clone, PartialEq)]
pub struct ChatResponse {
    /// What the call cost in US dollars, as recorded in the ledger: the
    /// provider's own figure, or the price table's. `None` until then.
    pub cost_usd: Option<f64>,
    /// The answer's text.
    pub content: String,
    /// The model that answered.
    pub model: String,
    /// Why it stopped (`stop`, `length`, …).
    pub finish_reason: Option<String>,
    /// Tokens used, when the provider says.
    pub usage: Option<Usage>,
}

impl std::fmt::Debug for ChatResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatResponse")
            .field("content_len", &self.content.len())
            .field("model", &self.model)
            .field("finish_reason", &self.finish_reason)
            .field("usage", &self.usage)
            .field("cost_usd", &self.cost_usd)
            .finish()
    }
}

impl ChatResponse {
    /// The answer parsed as JSON: plain, or in a ```json fence.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
        serde_json::from_str(unfenced(&self.content)).map_err(|e| {
            // Line and column only: the message can quote the answer.
            Error::InvalidResponse(format!(
                "the answer is not the expected JSON ({:?} at line {}, column {})",
                e.classify(),
                e.line(),
                e.column()
            ))
        })
    }
}

/// `content` without a surrounding Markdown code fence.
fn unfenced(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(inner) = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
    else {
        return trimmed;
    };
    let Some((info, body)) = inner.split_once('\n') else {
        return trimmed;
    };
    let info = info.trim();
    if info.is_empty() || info.eq_ignore_ascii_case("json") {
        body.trim()
    } else {
        trimmed
    }
}

/// Whose call this is: the ledger it is counted in and the cap it is held
/// to.
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    /// The database the ledger lives in.
    pub db: &'a PgPool,
    /// The account the call is for.
    pub account: Uuid,
    /// The time (the caller's clock).
    pub now: DateTime<Utc>,
}

/// What a person spent on AI, in euros, against their limits.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spend {
    /// Today (UTC).
    pub today_eur: f64,
    /// Their daily limit; `None` is none.
    pub day_limit_eur: Option<f64>,
    /// This month (UTC).
    pub month_eur: f64,
    /// Their monthly limit; `None` is none.
    pub month_limit_eur: Option<f64>,
}

/// What the whole instance spent this month, in euros.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InstanceSpend {
    /// This month (UTC).
    pub month_eur: f64,
    /// Today (UTC).
    pub today_eur: f64,
    /// The instance's monthly limit; `None` is none.
    pub limit_eur: Option<f64>,
    /// Calls this month.
    pub calls: i64,
    /// Accounts that made one.
    pub accounts: i64,
}

#[derive(Serialize)]
struct UsageRequest {
    include: bool,
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<&'a ResponseFormat>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<UsageRequest>,
}

#[derive(Deserialize)]
struct WireResponse {
    #[serde(default)]
    model: String,
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    /// OpenRouter: what the call cost, in USD credits.
    #[serde(default)]
    cost: Option<f64>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireMessage {
    content: Option<String>,
}

/// Calls models by role. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Gateway {
    config: AiConfig,
    http: Client,
    retry: RetryPolicy,
}

impl Gateway {
    /// A gateway over `config` with the default timeouts.
    pub fn new(config: AiConfig) -> Result<Self, Error> {
        Self::with_timeouts(config, DEFAULT_CONNECT_TIMEOUT, DEFAULT_REQUEST_TIMEOUT)
    }

    /// A gateway with these timeouts.
    pub fn with_timeouts(
        config: AiConfig,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, Error> {
        let http = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            // Never follow a redirect: the request carries a CV and a key.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("applicamp/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            config,
            http,
            retry: RetryPolicy::default(),
        })
    }

    /// The same gateway retrying with `retry`.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// The configuration it serves.
    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    /// Whether `role` has a provider.
    pub fn is_configured(&self, role: Role) -> bool {
        self.config.is_configured(role)
    }

    /// The price of `role`'s model, if it is configured.
    pub fn price(&self, role: Role) -> Option<Price> {
        self.config.provider(role).ok().map(price::price_of)
    }

    /// Euros per US dollar.
    pub fn usd_to_eur(&self) -> f64 {
        self.config.budget.usd_to_eur()
    }

    /// The spending limits, in US dollars.
    fn limits(&self) -> Limits {
        let budget = &self.config.budget;
        let rate = budget.usd_to_eur();
        Limits {
            account_day: budget.account_daily_eur().map(|eur| eur / rate),
            account_month: budget.account_monthly_eur().map(|eur| eur / rate),
            instance_month: budget.instance_monthly_eur().map(|eur| eur / rate),
        }
    }

    /// What `account` spent today and this month, against its limits.
    pub async fn spend(
        &self,
        db: &PgPool,
        account: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Spend, Error> {
        let spent = usage::spent(db, account, now)
            .await
            .map_err(|_| Error::Metering)?;
        let rate = self.usd_to_eur();
        let budget = &self.config.budget;
        Ok(Spend {
            today_eur: spent.account_day * rate,
            day_limit_eur: budget.account_daily_eur(),
            month_eur: spent.account_month * rate,
            month_limit_eur: budget.account_monthly_eur(),
        })
    }

    /// What the instance spent this month, against its limit.
    pub async fn instance_spend(
        &self,
        db: &PgPool,
        now: DateTime<Utc>,
    ) -> Result<InstanceSpend, Error> {
        let month = usage::instance_month(db, now)
            .await
            .map_err(|_| Error::Metering)?;
        let rate = self.usd_to_eur();
        Ok(InstanceSpend {
            month_eur: month.month_usd * rate,
            today_eur: month.today_usd * rate,
            limit_eur: self.config.budget.instance_monthly_eur(),
            calls: month.calls,
            accounts: month.accounts,
        })
    }

    /// One chat completion on the `complete` role, for `caller`: refused
    /// before anything is sent when the role is not configured
    /// ([`Error::NotConfigured`]), the account reached its daily token cap
    /// ([`Error::OverDailyCap`]) or the call's worst case (its prompt and its
    /// whole `max_tokens` answer at the model's price) would cross a
    /// spending limit ([`Error::OverBudget`]). Recorded in the ledger with
    /// its cost once answered; [`ChatResponse::cost_usd`] says what it was.
    pub async fn complete(
        &self,
        caller: Caller<'_>,
        request: ChatRequest,
    ) -> Result<ChatResponse, Error> {
        let role = Role::Complete;
        let provider = self.config.provider(role)?;
        if let Some(cap) = self.config.daily_token_cap() {
            let used = usage::used_today(caller.db, caller.account, caller.now)
                .await
                .map_err(|e| {
                    tracing::warn!(error = %e, %role, "AI usage could not be read; no call made");
                    Error::Metering
                })?;
            if used >= cap {
                tracing::info!(%role, cap, "an account reached its daily AI limit");
                return Err(Error::OverDailyCap { cap });
            }
        }
        let price = price::price_of(provider);
        let worst = price::worst_case(price, &request);
        let limits = self.limits();
        let reservation = match usage::reserve(
            caller.db,
            caller.account,
            role,
            &provider.model,
            worst,
            &limits,
            caller.now,
        )
        .await
        {
            Ok(Ok(reservation)) => reservation,
            Ok(Err(limit)) => {
                if limit == Limit::InstanceMonth {
                    tracing::error!(
                        %role,
                        limit = limit.as_str(),
                        "the instance reached its monthly AI spending limit: AI calls are refused"
                    );
                } else {
                    tracing::info!(%role, limit = limit.as_str(), "an account reached an AI spending limit");
                }
                return Err(Error::OverBudget { limit });
            }
            Err(e) => {
                tracing::warn!(error = %e, %role, "AI spending could not be read; no call made");
                return Err(Error::Metering);
            }
        };
        let mut response = match self.chat(provider, &request).await {
            Ok(response) => response,
            Err(err) => {
                self.unwind(caller.db, reservation, &err).await;
                return Err(err);
            }
        };
        let tokens = response.usage.unwrap_or_default();
        let model = if response.model.is_empty() {
            provider.model.clone()
        } else {
            response.model.clone()
        };
        let cost = response.cost_usd.unwrap_or_else(|| {
            price.cost(
                u64::from(tokens.prompt_tokens),
                u64::from(tokens.completion_tokens),
            )
        });
        if let Err(e) = usage::settle(
            caller.db,
            reservation,
            &model,
            tokens.prompt_tokens,
            tokens.completion_tokens,
            cost,
        )
        .await
        {
            // The reservation stays at the worst case: the limits still hold.
            tracing::warn!(error = %e, %role, "an AI call's cost could not be recorded");
        }
        response.cost_usd = Some(cost);
        tracing::info!(
            %role,
            model = %model,
            tokens_in = tokens.prompt_tokens,
            tokens_out = tokens.completion_tokens,
            cost_usd = cost,
            finish = response.finish_reason.as_deref().unwrap_or("none"),
            "ai call"
        );
        self.alert(caller.db, caller.now, cost).await;
        Ok(response)
    }

    /// After a failed call: release its reservation when it surely cost
    /// nothing (the provider refused it, or it was never reached); keep the
    /// worst case when it may have been charged (a timeout, an unreadable
    /// answer).
    async fn unwind(&self, db: &PgPool, reservation: Reservation, err: &Error) {
        let maybe_charged = match err {
            Error::Transport(e) => e.is_timeout() || e.is_body() || e.is_decode(),
            Error::InvalidResponse(_) => true,
            _ => false,
        };
        let done = if maybe_charged {
            usage::keep(db, reservation).await
        } else {
            usage::release(db, reservation).await
        };
        if let Err(e) = done {
            tracing::warn!(error = %e, "an AI reservation could not be settled");
        }
    }

    /// A log line when this call took the instance past 80 % or 100 % of
    /// its monthly limit (operations wire it to an alert).
    async fn alert(&self, db: &PgPool, now: DateTime<Utc>, cost_usd: f64) {
        let Some(limit) = self.limits().instance_month else {
            return;
        };
        let Ok(month) = usage::instance_month(db, now).await else {
            return;
        };
        let before = month.month_usd - cost_usd;
        for (share, percent) in [(1.0, 100_u8), (0.8, 80)] {
            let mark = limit * share;
            if before < mark && month.month_usd >= mark {
                tracing::warn!(
                    percent,
                    spent_usd = month.month_usd,
                    limit_usd = limit,
                    "the instance's AI spending reached {percent}% of its monthly limit"
                );
                break;
            }
        }
    }

    async fn chat(
        &self,
        provider: &ProviderConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, Error> {
        let body = WireRequest {
            model: &provider.model,
            messages: &request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_format: request.response_format.as_ref(),
            stream: false,
            usage: provider
                .include_usage
                .unwrap_or(true)
                .then_some(UsageRequest { include: true }),
        };
        let mut http = self
            .http
            .post(endpoint(&provider.base_url, "chat/completions")?)
            .json(&body);
        if let Some(key) = bearer(provider) {
            http = http.bearer_auth(key);
        }
        let wire: WireResponse = self.send_with_retry(http).await?;
        let choice = wire
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::InvalidResponse("no choices".into()))?;
        Ok(ChatResponse {
            content: choice.message.content.unwrap_or_default(),
            model: wire.model,
            finish_reason: choice.finish_reason,
            cost_usd: wire
                .usage
                .as_ref()
                .and_then(|u| u.cost)
                .filter(|c| c.is_finite() && *c >= 0.0),
            usage: wire.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
            }),
        })
    }

    async fn send_with_retry<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, Error> {
        let mut retry = 0;
        loop {
            let attempt = request
                .try_clone()
                .ok_or_else(|| Error::Config("the request cannot be retried".into()))?;
            let last = retry + 1 >= self.retry.attempts;
            let retry_after = match attempt.send().await {
                Ok(response) if retryable(response.status()) && !last => {
                    tracing::debug!(status = response.status().as_u16(), "ai provider busy");
                    retry_after(&response)
                }
                Ok(response) => return read(response).await,
                Err(err) if err.is_connect() && !last => None,
                Err(err) => return Err(err.into()),
            };
            let delay = self.retry.delay(retry, retry_after);
            retry += 1;
            tracing::debug!(
                retry,
                delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                "retrying the ai provider"
            );
            tokio::time::sleep(delay).await;
        }
    }
}

/// 429 (busy) and 5xx (failing) are worth another attempt.
fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// `path` under `base_url`, which keeps its own path (`…/api/v1`).
fn endpoint(base_url: &Url, path: &str) -> Result<Url, Error> {
    let mut base = base_url.clone();
    if !base.path().ends_with('/') {
        let with_slash = format!("{}/", base.path());
        base.set_path(&with_slash);
    }
    base.join(path)
        .map_err(|e| Error::Config(format!("invalid base_url {base_url}: {e}")))
}

/// A `Retry-After` in seconds, on a 429 or 503.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let status = response.status();
    if status != StatusCode::TOO_MANY_REQUESTS && status != StatusCode::SERVICE_UNAVAILABLE {
        return None;
    }
    let seconds = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    seconds.trim().parse::<u64>().ok().map(Duration::from_secs)
}

async fn read<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, Error> {
    let status = response.status();
    if !status.is_success() {
        drop(response.bytes().await);
        return Err(Error::Status {
            status: status.as_u16(),
        });
    }
    let bytes = response.bytes().await?;
    serde_json::from_slice(&bytes).map_err(|e| {
        Error::InvalidResponse(format!(
            "not a chat completion ({:?} at line {}, column {})",
            e.classify(),
            e.line(),
            e.column()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retry_delay_doubles_up_to_the_cap_and_honours_retry_after() {
        let policy = RetryPolicy {
            attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
        };
        assert_eq!(policy.delay(0, None), Duration::from_millis(100));
        assert_eq!(policy.delay(1, None), Duration::from_millis(200));
        assert_eq!(policy.delay(5, None), Duration::from_secs(1));
        assert_eq!(
            policy.delay(0, Some(Duration::from_millis(500))),
            Duration::from_millis(500)
        );
        assert_eq!(
            policy.delay(0, Some(Duration::from_secs(60))),
            Duration::from_secs(1),
            "a huge Retry-After is capped"
        );
    }

    #[test]
    fn the_endpoint_keeps_the_base_path() {
        for base in [
            "https://openrouter.example.com/api/v1",
            "https://openrouter.example.com/api/v1/",
        ] {
            assert_eq!(
                endpoint(&Url::parse(base).unwrap(), "chat/completions")
                    .unwrap()
                    .as_str(),
                "https://openrouter.example.com/api/v1/chat/completions"
            );
        }
    }

    fn answer(content: &str) -> ChatResponse {
        ChatResponse {
            content: content.to_owned(),
            model: "m".to_owned(),
            finish_reason: Some("stop".to_owned()),
            usage: None,
            cost_usd: None,
        }
    }

    #[test]
    fn json_reads_plain_and_fenced_answers() {
        let expected = serde_json::json!({ "a": 1 });
        for content in [
            r#"{"a": 1}"#,
            "\n{\"a\": 1}\n",
            "```json\n{\"a\": 1}\n```",
            "```\n{\"a\": 1}\n```",
        ] {
            let value: serde_json::Value = answer(content).json().unwrap();
            assert_eq!(value, expected, "{content:?}");
        }
        let err = answer("Sure! Here is your CV.")
            .json::<serde_json::Value>()
            .unwrap_err();
        assert!(!err.to_string().contains("Sure"), "{err}");
    }

    #[test]
    fn debug_never_shows_content() {
        let message = Message::user("a person's whole CV");
        assert!(!format!("{message:?}").contains("CV"));
        assert!(!format!("{:?}", answer("their CV")).contains("CV"));
    }
}
