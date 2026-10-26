//! `applicamp-ai`: the model gateway.
//!
//! Every model call goes through [`Gateway`] by **role**, never by vendor.
//! Applicamp has one role so far, [`Role::Complete`] (structuring, writing),
//! served by any provider speaking the OpenAI-compatible chat completions
//! API: OpenRouter, or a self-hosted vLLM / llama.cpp / Ollama.
//!
//! A trimmed copy of Campbooks Next's `campbooks-ai`: no `decide`, `embed` or
//! `vision` roles, no streaming, no tools. What it keeps:
//!
//! - timeouts, and retries with backoff on 429 and 5xx answers;
//! - a usage ledger (`ai.usage`): every call's tokens and cost, per account;
//! - a per-account daily token cap (`APPLICAMP_AI__DAILY_TOKENS`);
//! - spending limits in money, checked before each call against its worst
//!   case: per account per day and per month, and for the whole instance
//!   per month (`APPLICAMP_AI__BUDGET__…`, [`BudgetConfig`]).
//!
//! Logs and errors carry the role, the model, statuses and token counts,
//! never a prompt or an answer.

#![forbid(unsafe_code)]

mod config;
mod error;
mod gateway;
pub mod price;
pub mod usage;

#[cfg(test)]
mod tests;

pub use config::{
    AiConfig, BudgetConfig, DEFAULT_ACCOUNT_DAILY_EUR, DEFAULT_ACCOUNT_MONTHLY_EUR,
    DEFAULT_DAILY_TOKENS, DEFAULT_INSTANCE_MONTHLY_EUR, DEFAULT_USD_TO_EUR, ProviderConfig, Role,
};
pub use error::{AI_BUDGET_REACHED, AI_DAILY_LIMIT_REACHED, AI_FAILED, AI_UNAVAILABLE, Error};
pub use gateway::{
    Caller, ChatRequest, ChatResponse, DEFAULT_CONNECT_TIMEOUT, DEFAULT_REQUEST_TIMEOUT, Gateway,
    InstanceSpend, JsonSchema, Message, MessageRole, ResponseFormat, RetryPolicy, Spend, Usage,
};
pub use price::Price;
