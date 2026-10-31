//! What "Grill me" trusts of the model's answer. The model proposes facts,
//! what the answer covered and the next question; the rules here decide, with
//! no model and no keywords:
//!
//! - a fact's **quote** must be a verbatim part of the person's answer (the
//!   import source check's comparison: no case, no whitespace, plain quotes
//!   and dashes);
//! - a fact's **text** must pass the draft source check against that quote
//!   and the profile: no number, name or tool neither says;
//! - a fact that fails either check, or names an area that does not exist, is
//!   left out and counted, never shown;
//! - coverage only goes up (`none` < `part` < `ok`); areas that do not exist
//!   are ignored;
//! - the next question is asked only while something is left to ask.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    features::{completion::check::Evidence, imports::source_check::squash},
    schema::Area,
};

/// Most facts one answer yields.
pub const MAX_FACTS_PER_ANSWER: usize = 5;
/// Longest fact, in characters.
pub const MAX_FACT: usize = 300;
/// Longest quote, in characters.
pub const MAX_QUOTE: usize = 600;
/// Longest question, in characters.
pub const MAX_QUESTION: usize = 600;

/// How well an area is covered.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Nothing yet.
    #[default]
    None,
    /// Some of it.
    Part,
    /// Enough.
    Ok,
}

impl Level {
    /// The level the model names; `none` and anything else is no news.
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "part" => Some(Self::Part),
            "ok" => Some(Self::Ok),
            _ => None,
        }
    }
}

/// What has been covered about a position.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage(pub BTreeMap<Area, Level>);

impl Coverage {
    /// How well `area` is covered.
    pub fn level(&self, area: Area) -> Level {
        self.0.get(&area).copied().unwrap_or_default()
    }

    /// Raise `area` to `level`; it never goes down. Whether it changed.
    pub fn raise(&mut self, area: Area, level: Level) -> bool {
        if level > self.level(area) {
            self.0.insert(area, level);
            true
        } else {
            false
        }
    }

    /// Areas at `ok`.
    pub fn ok_count(&self) -> u8 {
        let ok = Area::ALL
            .into_iter()
            .filter(|area| self.level(*area) == Level::Ok)
            .count();
        u8::try_from(ok).unwrap_or(u8::MAX)
    }

    /// Whether every area is at `ok`.
    pub fn all_ok(&self) -> bool {
        usize::from(self.ok_count()) == Area::ALL.len()
    }

    /// The first area, in order, that is not `ok` and was not skipped.
    pub fn first_open(&self, skipped: &[Area]) -> Option<Area> {
        Area::ALL
            .into_iter()
            .find(|area| self.level(*area) != Level::Ok && !skipped.contains(area))
    }
}

/// The model's answer, as it comes. Everything is read leniently: what is
/// missing is empty, and what is unknown is ignored by [`judge`].
#[derive(Debug, Default, Deserialize)]
pub struct Raw {
    #[serde(default)]
    facts: Vec<RawFact>,
    #[serde(default)]
    coverage: Vec<RawCoverage>,
    #[serde(default)]
    next: Option<RawNext>,
    #[serde(default)]
    conflict: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawFact {
    #[serde(default)]
    text: String,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    area: String,
}

#[derive(Debug, Deserialize)]
struct RawCoverage {
    #[serde(default)]
    area: String,
    #[serde(default)]
    level: String,
}

#[derive(Debug, Deserialize)]
struct RawNext {
    #[serde(default)]
    question: String,
    #[serde(default)]
    area: String,
    #[serde(default)]
    push: bool,
}

/// A fact that passed the checks and waits for the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    pub text: String,
    pub quote: String,
    pub area: Area,
}

/// The next question to ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextQuestion {
    pub question: String,
    pub area: Area,
    /// It pushes back, or asks about a contradiction.
    pub push: bool,
}

/// What the model's answer comes to once the rules have run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub facts: Vec<NewFact>,
    /// Facts left out by a check.
    pub dropped: u32,
    pub coverage: Coverage,
    pub next: Option<NextQuestion>,
}

/// What the rules need to know besides the model's answer.
pub struct Input<'a> {
    /// The person's answer; none when the model was asked for a first or a
    /// next question (start, skip): then it can yield no facts and no
    /// coverage.
    pub answer: Option<&'a str>,
    /// The profile, for the fact check.
    pub profile: &'a Value,
    /// Coverage before this answer.
    pub coverage: &'a Coverage,
    /// Areas the person skipped.
    pub skipped: &'a [Area],
    /// The area of the question being answered.
    pub asked: Option<Area>,
    /// Facts already kept or waiting, squashed: not proposed twice.
    pub known: &'a [String],
}

/// Whether `quote` is part of `answer`, word for word.
pub fn quote_is_verbatim(answer: &str, quote: &str) -> bool {
    let quote = squash(quote);
    !quote.is_empty() && squash(answer).contains(&quote)
}

/// Run the rules over the model's answer.
pub fn judge(raw: Raw, input: &Input<'_>) -> Outcome {
    let mut facts: Vec<NewFact> = Vec::new();
    let mut dropped = 0_u32;
    let mut coverage = input.coverage.clone();

    if let Some(answer) = input.answer {
        let total = raw.facts.len();
        for fact in raw.facts.into_iter().take(MAX_FACTS_PER_ANSWER) {
            let text = fact.text.trim().to_owned();
            let quote = fact.quote.trim().to_owned();
            let Some(area) = Area::parse(&fact.area) else {
                dropped += 1;
                continue;
            };
            let fits = !text.is_empty()
                && !quote.is_empty()
                && text.chars().count() <= MAX_FACT
                && quote.chars().count() <= MAX_QUOTE;
            if !fits
                || !quote_is_verbatim(answer, &quote)
                || !Evidence::of(&quote, input.profile)
                    .unsupported(&text)
                    .is_empty()
            {
                dropped += 1;
                continue;
            }
            let key = squash(&text);
            if input.known.contains(&key) || facts.iter().any(|f| squash(&f.text) == key) {
                continue;
            }
            facts.push(NewFact { text, quote, area });
        }
        dropped += u32::try_from(total.saturating_sub(MAX_FACTS_PER_ANSWER)).unwrap_or(u32::MAX);

        for entry in raw.coverage {
            if let (Some(area), Some(level)) =
                (Area::parse(&entry.area), Level::parse(&entry.level))
            {
                coverage.raise(area, level);
            }
        }
    }

    let open = coverage.first_open(input.skipped);
    let conflict = raw
        .conflict
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty() && c.chars().count() <= MAX_QUESTION);
    let next = if let Some(question) = conflict {
        Some(NextQuestion {
            question,
            area: input.asked.or(open).unwrap_or(Area::Context),
            push: true,
        })
    } else if open.is_none() {
        // Everything is covered or skipped: nothing left to ask.
        None
    } else {
        raw.next.and_then(|next| {
            let question = next.question.trim().to_owned();
            (!question.is_empty() && question.chars().count() <= MAX_QUESTION).then(|| {
                NextQuestion {
                    question,
                    area: Area::parse(&next.area).or(open).unwrap_or(Area::Context),
                    push: next.push,
                }
            })
        })
    };
    Outcome {
        facts,
        dropped,
        coverage,
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ANSWER: &str = "I proposed it and led it. 2 others helped part time. It took 7 months.";

    fn raw(value: &Value) -> Raw {
        serde_json::from_value(value.clone()).unwrap()
    }

    fn input<'a>(answer: Option<&'a str>, profile: &'a Value, coverage: &'a Coverage) -> Input<'a> {
        Input {
            answer,
            profile,
            coverage,
            skipped: &[],
            asked: Some(Area::YourPart),
            known: &[],
        }
    }

    #[test]
    fn a_quote_must_be_the_persons_words_whatever_their_spacing_or_quotes() {
        assert!(quote_is_verbatim(ANSWER, "2 others helped part time"));
        assert!(quote_is_verbatim(ANSWER, "  2 OTHERS  helped\npart time "));
        assert!(quote_is_verbatim(
            "We called it \u{201c}Project X\u{201d} \u{2014} a rewrite",
            "\"project x\" - a rewrite"
        ));
        assert!(!quote_is_verbatim(ANSWER, "2 others helped full time"));
        assert!(!quote_is_verbatim(ANSWER, "I saved 30%"));
        assert!(!quote_is_verbatim(ANSWER, ""));
        assert!(!quote_is_verbatim(ANSWER, "   "));
    }

    #[test]
    fn facts_with_a_made_up_quote_or_a_claim_nobody_made_are_left_out_and_counted() {
        let profile = json!({ "work": [{ "name": "Example Org" }] });
        let coverage = Coverage::default();
        let outcome = judge(
            raw(&json!({
                "facts": [
                    { "text": "Led the upgrade", "quote": "I proposed it and led it", "area": "your_part" },
                    // The quote was never said.
                    { "text": "Saved 30% of cost", "quote": "we saved 30%", "area": "results" },
                    // A real quote, but the text claims 9 months.
                    { "text": "Took 9 months", "quote": "It took 7 months", "area": "results" },
                    // An area that does not exist.
                    { "text": "Took 7 months", "quote": "It took 7 months", "area": "vibes" },
                    { "text": "Took 7 months", "quote": "It took 7 months", "area": "results" },
                ],
                "coverage": [], "next": null, "conflict": null,
            })),
            &input(Some(ANSWER), &profile, &coverage),
        );
        let texts: Vec<&str> = outcome.facts.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, ["Led the upgrade", "Took 7 months"]);
        assert_eq!(outcome.dropped, 3);
    }

    #[test]
    fn what_the_profile_says_counts_as_evidence_for_a_fact() {
        let profile = json!({ "work": [{ "name": "Example Org" }] });
        let coverage = Coverage::default();
        let outcome = judge(
            raw(&json!({
                "facts": [
                    { "text": "Led the upgrade at Example Org", "quote": "I led it", "area": "your_part" },
                    { "text": "Led the upgrade at Acme", "quote": "I led it", "area": "your_part" },
                ],
            })),
            &input(Some("I led it, with 4 others"), &profile, &coverage),
        );
        assert_eq!(outcome.facts.len(), 1);
        assert_eq!(outcome.dropped, 1);
    }

    #[test]
    fn a_fact_already_kept_or_waiting_is_not_proposed_twice() {
        let profile = json!({});
        let coverage = Coverage::default();
        let known = [squash("Took 7 months")];
        let outcome = judge(
            raw(&json!({ "facts": [
                { "text": "Took 7 months", "quote": "It took 7 months", "area": "results" },
                { "text": "took 7  months", "quote": "It took 7 months", "area": "results" },
            ] })),
            &Input {
                known: &known,
                ..input(Some(ANSWER), &profile, &coverage)
            },
        );
        assert!(outcome.facts.is_empty());
        assert_eq!(outcome.dropped, 0);
    }

    #[test]
    fn coverage_only_ever_goes_up() {
        let profile = json!({});
        let mut before = Coverage::default();
        before.raise(Area::Context, Level::Ok);
        before.raise(Area::Results, Level::Part);
        let outcome = judge(
            raw(&json!({
                "coverage": [
                    { "area": "context", "level": "part" },
                    { "area": "context", "level": "none" },
                    { "area": "results", "level": "ok" },
                    { "area": "people", "level": "part" },
                    { "area": "nonsense", "level": "ok" },
                    { "area": "stories", "level": "great" },
                ],
                "next": { "question": "And the team?", "area": "people", "push": false },
            })),
            &input(Some(ANSWER), &profile, &before),
        );
        assert_eq!(outcome.coverage.level(Area::Context), Level::Ok);
        assert_eq!(outcome.coverage.level(Area::Results), Level::Ok);
        assert_eq!(outcome.coverage.level(Area::People), Level::Part);
        assert_eq!(outcome.coverage.level(Area::Stories), Level::None);
        assert_eq!(outcome.coverage.0.len(), 3, "unknown areas are ignored");
        assert!(!Coverage::default().clone().raise(Area::People, Level::None));
        let mut c = Coverage::default();
        assert!(c.raise(Area::People, Level::Ok));
        assert!(!c.raise(Area::People, Level::Part), "never down");
        assert_eq!(c.level(Area::People), Level::Ok);
    }

    #[test]
    fn a_first_or_next_question_yields_no_facts_and_no_coverage() {
        let profile = json!({});
        let coverage = Coverage::default();
        let outcome = judge(
            raw(&json!({
                "facts": [{ "text": "Led it", "quote": "I led it", "area": "your_part" }],
                "coverage": [{ "area": "context", "level": "ok" }],
                "next": { "question": "What did the company sell?", "area": "context", "push": false },
            })),
            &input(None, &profile, &coverage),
        );
        assert!(outcome.facts.is_empty());
        assert_eq!(outcome.dropped, 0);
        assert_eq!(outcome.coverage, Coverage::default());
        assert_eq!(
            outcome.next,
            Some(NextQuestion {
                question: "What did the company sell?".to_owned(),
                area: Area::Context,
                push: false,
            })
        );
    }

    #[test]
    fn nothing_is_asked_once_everything_is_covered_or_skipped() {
        let profile = json!({});
        let mut full = Coverage::default();
        for area in Area::ALL {
            full.raise(area, Level::Ok);
        }
        let ask =
            json!({ "next": { "question": "Anything else?", "area": "stories", "push": false } });
        assert_eq!(
            judge(raw(&ask), &input(Some(ANSWER), &profile, &full)).next,
            None
        );

        let mut most = Coverage::default();
        for area in &Area::ALL[..6] {
            most.raise(*area, Level::Ok);
        }
        let skipped = [Area::Stories];
        let outcome = judge(
            raw(&ask),
            &Input {
                skipped: &skipped,
                ..input(Some(ANSWER), &profile, &most)
            },
        );
        assert_eq!(outcome.next, None, "the one area left was skipped");
    }

    #[test]
    fn a_contradiction_becomes_a_pushing_question_even_when_all_is_covered() {
        let profile = json!({});
        let mut full = Coverage::default();
        for area in Area::ALL {
            full.raise(area, Level::Ok);
        }
        let outcome = judge(
            raw(&json!({
                "next": null,
                "conflict": "You said 7 months before, and now 12. Which is right?",
            })),
            &input(Some(ANSWER), &profile, &full),
        );
        let next = outcome.next.unwrap();
        assert!(next.push);
        assert_eq!(next.area, Area::YourPart);
        assert!(next.question.starts_with("You said 7 months"));
    }

    #[test]
    fn an_empty_or_overlong_question_is_no_question() {
        let profile = json!({});
        let coverage = Coverage::default();
        let long = "x".repeat(MAX_QUESTION + 1);
        for question in ["", "   ", long.as_str()] {
            let outcome = judge(
                raw(&json!({ "next": { "question": question, "area": "context", "push": false } })),
                &input(None, &profile, &coverage),
            );
            assert_eq!(outcome.next, None);
        }
        // An area the model invented falls back to the first open one.
        let outcome = judge(
            raw(
                &json!({ "next": { "question": "What did it sell?", "area": "x", "push": false } }),
            ),
            &input(None, &profile, &coverage),
        );
        assert_eq!(outcome.next.unwrap().area, Area::Context);
    }

    #[test]
    fn what_is_missing_from_the_models_object_is_empty_and_bad_json_is_an_error() {
        let bare: Raw = serde_json::from_str("{}").unwrap();
        assert!(bare.facts.is_empty() && bare.next.is_none());
        assert!(serde_json::from_str::<Raw>("not json").is_err());
        assert!(serde_json::from_str::<Raw>("{\"facts\": 3}").is_err());
        assert!(serde_json::from_str::<Raw>("").is_err());
    }

    #[test]
    fn at_most_five_facts_come_from_one_answer() {
        let profile = json!({});
        let coverage = Coverage::default();
        let facts: Vec<Value> = (0..8)
            .map(|n| {
                json!({
                    "text": format!("Took {n} months"),
                    "quote": "It took 7 months",
                    "area": "results",
                })
            })
            .collect();
        let outcome = judge(
            raw(&json!({ "facts": facts })),
            &input(Some("It took 7 months, 0 1 2 3 4 5 6"), &profile, &coverage),
        );
        assert!(outcome.facts.len() <= MAX_FACTS_PER_ANSWER);
        assert!(outcome.dropped >= 3);
    }
}
