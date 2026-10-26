//! Goals: what a person wants next, as typed answers to a short interview.
//!
//! Every answer is optional: a list with nothing in it, or a missing value,
//! is a question not answered. The questions are fixed and translated by the
//! client; these types are the API's (`utoipa`) and what is stored
//! (`profile.goals.answers`). Unknown fields and unknown choices are refused.

use std::sync::LazyLock;

use applicamp_kernel::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Most roles a person can name.
pub const MAX_ROLES: usize = 5;
/// Most places, things to avoid and must-haves.
pub const MAX_LIST: usize = 10;
/// Most languages to work in.
pub const MAX_LANGUAGES: usize = 20;
/// Longest item of a list, in characters.
pub const MAX_ITEM: usize = 100;
/// Largest cash floor, in whole units of its currency.
pub const MAX_AMOUNT: u32 = 100_000_000;

/// The active ISO 4217 currency codes a cash floor may use.
pub const CURRENCIES: [&str; 154] = [
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD",
    "CDF", "CHF", "CLP", "CNY", "COP", "CRC", "CUP", "CVE", "CZK", "DJF", "DKK", "DOP", "DZD",
    "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ",
    "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD",
    "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR",
    "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP", "MRU", "MUR", "MVR",
    "MWK", "MXN", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN",
    "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR",
    "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL", "THB",
    "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "UYU", "UZS",
    "VES", "VND", "VUV", "WST", "XAF", "XCD", "XOF", "XPF", "YER", "ZAR", "ZMW",
];

static LANGUAGE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Which levels fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Seniority {
    Junior,
    Mid,
    Senior,
    Staff,
    Principal,
    Lead,
    Manager,
    Director,
    Founding,
}

/// How they want to work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkMode {
    Remote,
    Hybrid,
    Onsite,
}

/// Whether they would move for the right job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Relocate {
    /// No.
    No,
    /// Within their country.
    WithinCountry,
    /// Within the EU.
    WithinEu,
    /// Anywhere.
    Anywhere,
}

/// What a cash floor is counted over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Year,
    Month,
}

/// The least they would take, before tax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CashFloor {
    /// Whole units of `currency` per `period`, 1 to 100 000 000.
    pub amount: u32,
    /// An ISO 4217 code such as `EUR`.
    pub currency: String,
    /// Per year or per month.
    pub period: Period,
}

/// What kind of company.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanyStage {
    Early,
    Growth,
    Established,
    Public,
    Nonprofit,
}

/// How much agents may do alone. Applying and emailing always wait for the
/// person, whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    /// Agents only find jobs.
    FindOnly,
    /// Agents also prepare CVs and applications.
    Prepare,
    /// Agents also schedule the work.
    PrepareAndSchedule,
}

/// The answers to the goals interview. Nothing is required.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Goals {
    /// The roles they want next (up to 5).
    #[serde(default)]
    #[schema(required = true)]
    pub roles: Vec<String>,
    /// The levels that fit.
    #[serde(default)]
    #[schema(required = true)]
    pub seniority: Vec<Seniority>,
    /// How they want to work.
    #[serde(default)]
    #[schema(required = true)]
    pub work_mode: Vec<WorkMode>,
    /// Where they can work from: cities, countries or regions (up to 10).
    #[serde(default)]
    #[schema(required = true)]
    pub locations: Vec<String>,
    /// Whether they would move for the right job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocate: Option<Relocate>,
    /// The least they would take, before tax.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash_floor: Option<CashFloor>,
    /// Whether equity counts toward the cash floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equity_counts: Option<bool>,
    /// The kinds of company.
    #[serde(default)]
    #[schema(required = true)]
    pub company_stage: Vec<CompanyStage>,
    /// What they will not work on (up to 10).
    #[serde(default)]
    #[schema(required = true)]
    pub avoid: Vec<String>,
    /// What the job must have (up to 10).
    #[serde(default)]
    #[schema(required = true)]
    pub must_have: Vec<String>,
    /// The languages they can work in, as BCP 47 tags (`en`, `pt-PT`).
    #[serde(default)]
    #[schema(required = true)]
    pub languages_for_work: Vec<String>,
    /// How much agents may do alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<Autonomy>,
}

fn tidy_texts(items: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::new();
    items.retain_mut(|item| {
        *item = item.split_whitespace().collect::<Vec<_>>().join(" ");
        let key = item.to_lowercase();
        let keep = !item.is_empty() && !seen.contains(&key);
        if keep {
            seen.push(key);
        }
        keep
    });
}

fn dedupe<T: PartialEq + Copy>(items: &mut Vec<T>) {
    let mut kept: Vec<T> = Vec::with_capacity(items.len());
    for item in items.iter() {
        if !kept.contains(item) {
            kept.push(*item);
        }
    }
    *items = kept;
}

fn invalid(detail: String) -> Error {
    Error::BadRequest("invalid_goals", detail)
}

impl Goals {
    /// The answers trimmed: blank items and repeats removed, a currency in
    /// capitals, choices listed once.
    pub fn tidied(mut self) -> Self {
        for list in [
            &mut self.roles,
            &mut self.locations,
            &mut self.avoid,
            &mut self.must_have,
            &mut self.languages_for_work,
        ] {
            tidy_texts(list);
        }
        dedupe(&mut self.seniority);
        dedupe(&mut self.work_mode);
        dedupe(&mut self.company_stage);
        if let Some(floor) = &mut self.cash_floor {
            floor.currency = floor.currency.trim().to_uppercase();
        }
        self
    }

    /// Check the answers against their limits: the first problem, in
    /// English, as a 400 `invalid_goals`.
    pub fn validate(&self) -> Result<()> {
        for (name, list, most) in [
            ("roles", &self.roles, MAX_ROLES),
            ("locations", &self.locations, MAX_LIST),
            ("avoid", &self.avoid, MAX_LIST),
            ("must_have", &self.must_have, MAX_LIST),
        ] {
            if list.len() > most {
                return Err(invalid(format!("`{name}` takes at most {most} items.")));
            }
            if list.iter().any(|item| item.chars().count() > MAX_ITEM) {
                return Err(invalid(format!(
                    "An item of `{name}` is longer than {MAX_ITEM} characters."
                )));
            }
        }
        if self.languages_for_work.len() > MAX_LANGUAGES {
            return Err(invalid(format!(
                "`languages_for_work` takes at most {MAX_LANGUAGES} items."
            )));
        }
        if let Some(tag) = self
            .languages_for_work
            .iter()
            .find(|tag| tag.len() > 35 || !LANGUAGE_TAG.is_match(tag))
        {
            return Err(invalid(format!(
                "`{tag}` is not a language tag like `en` or `pt-PT`."
            )));
        }
        if let Some(floor) = &self.cash_floor {
            if floor.amount == 0 || floor.amount > MAX_AMOUNT {
                return Err(invalid(format!(
                    "`cash_floor.amount` is between 1 and {MAX_AMOUNT}."
                )));
            }
            if !CURRENCIES.contains(&floor.currency.as_str()) {
                return Err(invalid(format!(
                    "`{}` is not an ISO 4217 currency code like `EUR`.",
                    floor.currency
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_currency_list_is_sorted_and_three_capitals() {
        assert!(CURRENCIES.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            CURRENCIES
                .iter()
                .all(|code| code.len() == 3 && code.chars().all(|c| c.is_ascii_uppercase()))
        );
        assert!(CURRENCIES.contains(&"EUR"));
    }

    #[test]
    fn tidying_removes_blanks_and_repeats_and_capitalises_the_currency() {
        let goals = Goals {
            roles: vec![" Designer ".into(), "designer".into(), "  ".into()],
            seniority: vec![Seniority::Mid, Seniority::Mid],
            cash_floor: Some(CashFloor {
                amount: 1,
                currency: " usd".into(),
                period: Period::Month,
            }),
            ..Goals::default()
        }
        .tidied();
        assert_eq!(goals.roles, ["Designer"]);
        assert_eq!(goals.seniority, [Seniority::Mid]);
        assert_eq!(goals.cash_floor.map(|f| f.currency).as_deref(), Some("USD"));
    }

    #[test]
    fn empty_goals_are_valid() {
        assert!(Goals::default().validate().is_ok());
    }
}
