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
//! - a usage ledger (`ai.usage`): every call's tokens, per account;
//! - a per-account daily token cap (`APPLICAMP_AI__DAILY_TOKENS`).
//!
//! Logs and errors carry the role, the model, statuses and token counts,
//! never a prompt or an answer.

#![forbid(unsafe_code)]

mod config;
mod error;
mod gateway;
pub mod usage;

#[cfg(test)]
mod tests;

pub use config::{AiConfig, DEFAULT_DAILY_TOKENS, ProviderConfig, Role};
pub use error::{AI_DAILY_LIMIT_REACHED, AI_FAILED, AI_UNAVAILABLE, Error};
pub use gateway::{
    Caller, ChatRequest, ChatResponse, DEFAULT_CONNECT_TIMEOUT, DEFAULT_REQUEST_TIMEOUT, Gateway,
    JsonSchema, Message, MessageRole, ResponseFormat, RetryPolicy, Usage,
};
