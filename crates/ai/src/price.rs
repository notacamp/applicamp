//! What a model call costs, in US dollars.
//!
//! The provider says, when it can: OpenRouter answers each call with its
//! `usage.cost` in USD credits, and that is what the ledger records. This
//! table is the fallback for an answer without a cost, and the price the
//! gateway checks a call's **worst case** against before sending it. A
//! model the table does not know is priced high ([`UNKNOWN`]) so the limits
//! hold anyway; set `APPLICAMP_AI__COMPLETE__PRICE_IN_USD_PER_MTOK` and
//! `…_PRICE_OUT_USD_PER_MTOK` to price it exactly.

use crate::{ChatRequest, ProviderConfig};

/// A model's price, in US dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    /// Per million prompt tokens.
    pub in_per_mtok: f64,
    /// Per million completion tokens.
    pub out_per_mtok: f64,
}

impl Price {
    /// The cost of `tokens_in` prompt and `tokens_out` completion tokens.
    pub fn cost(&self, tokens_in: u64, tokens_out: u64) -> f64 {
        // Token counts are far below 2^52: exact as f64.
        let (tokens_in, tokens_out) = (tokens_in as f64, tokens_out as f64);
        (tokens_in * self.in_per_mtok + tokens_out * self.out_per_mtok) / 1_000_000.0
    }
}

/// Known models, as OpenRouter names them.
///
/// `z-ai/glm-5.3-flash`: OpenRouter's listing (`/api/v1/models`, `pricing`),
/// read on 2026-10-26.
pub const TABLE: [(&str, Price); 1] = [(
    "z-ai/glm-5.3-flash",
    Price {
        in_per_mtok: GLM_IN,
        out_per_mtok: GLM_OUT,
    },
)];

/// $0.000 000 15 a prompt token.
const GLM_IN: f64 = 0.15;
/// $0.000 000 5 a completion token.
const GLM_OUT: f64 = 0.50;

/// The price assumed for a model nobody priced: high, so a limit is never
/// crossed by a model that turns out dearer than thought.
pub const UNKNOWN: Price = Price {
    in_per_mtok: 3.0,
    out_per_mtok: 15.0,
};

/// The longest answer assumed when a request sets no `max_tokens`.
pub const ASSUMED_MAX_TOKENS: u32 = 8_000;

/// The price of `provider`'s model: its configured price, else the table's,
/// else [`UNKNOWN`]. A variant suffix (`:free`, `:nitro`) is ignored.
pub fn price_of(provider: &ProviderConfig) -> Price {
    let base = provider
        .model
        .split_once(':')
        .map_or(provider.model.as_str(), |(model, _)| model);
    let known = TABLE
        .iter()
        .find(|(model, _)| model.eq_ignore_ascii_case(base))
        .map_or(UNKNOWN, |(_, price)| *price);
    Price {
        in_per_mtok: provider.price_in_usd_per_mtok.unwrap_or(known.in_per_mtok),
        out_per_mtok: provider
            .price_out_usd_per_mtok
            .unwrap_or(known.out_per_mtok),
    }
}

/// Prompt tokens a request is assumed to use: a character for every three
/// and a bit for each message, more than any tokenizer counts for text.
pub fn prompt_tokens_estimate(request: &ChatRequest) -> u64 {
    let chars: usize = request
        .messages
        .iter()
        .map(|m| m.content.chars().count() + 16)
        .sum();
    u64::try_from(chars.div_ceil(3)).unwrap_or(u64::MAX)
}

/// The most `request` can cost at `price`: its prompt as estimated and the
/// whole `max_tokens` answer.
pub fn worst_case(price: Price, request: &ChatRequest) -> f64 {
    price.cost(
        prompt_tokens_estimate(request),
        u64::from(request.max_tokens.unwrap_or(ASSUMED_MAX_TOKENS)),
    )
}

#[cfg(test)]
mod tests {
    use crate::Message;

    use super::*;

    fn provider(model: &str) -> ProviderConfig {
        ProviderConfig::new(
            "https://openrouter.example.com/api/v1".parse().unwrap(),
            model,
        )
    }

    #[test]
    fn the_table_prices_known_models_and_unknown_ones_high() {
        assert_eq!(
            price_of(&provider("z-ai/glm-5.3-flash")).out_per_mtok,
            GLM_OUT
        );
        assert_eq!(
            price_of(&provider("z-ai/glm-5.3-flash:nitro")).in_per_mtok,
            GLM_IN
        );
        assert_eq!(price_of(&provider("someone/new-model")), UNKNOWN);
        let priced = provider("someone/new-model").with_price(1.0, 2.0);
        assert_eq!(
            price_of(&priced),
            Price {
                in_per_mtok: 1.0,
                out_per_mtok: 2.0
            }
        );
    }

    #[test]
    fn the_worst_case_counts_the_whole_answer() {
        let price = Price {
            in_per_mtok: 1.0,
            out_per_mtok: 10.0,
        };
        assert!((price.cost(1_000_000, 0) - 1.0).abs() < 1e-12);
        let request = ChatRequest {
            max_tokens: Some(1_000),
            ..ChatRequest::new(vec![Message::user("x".repeat(2_984))])
        };
        // (2 984 + 16) / 3 = 1 000 prompt tokens, and 1 000 out.
        assert_eq!(prompt_tokens_estimate(&request), 1_000);
        assert!((worst_case(price, &request) - 0.011).abs() < 1e-12);
    }
}
