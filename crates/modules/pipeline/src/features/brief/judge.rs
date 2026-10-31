//! What the brief trusts of the model's answers. The model proposes; these
//! rules decide, with no model and no keywords:
//!
//! - a company fact's **quote** must be a verbatim part of the text of the
//!   page it cites, and that page must be one that was read; otherwise the
//!   fact is left out and counted;
//! - a want's **quote** must be a verbatim part of the posting;
//! - a question is kept only if it is short, not about a skill the person
//!   dismissed, and not a repeat; at most [`MOST_QUESTIONS`], and an `about`
//!   that is not a position of the profile becomes none.

use std::collections::HashMap;

use serde::Deserialize;

use crate::ports::ProfileDigest;

/// Most facts a read keeps.
pub const MOST_FACTS: usize = 12;
/// Most things a job is said to want.
pub const MOST_WANTS: usize = 6;
/// Most questions asked about a job.
pub const MOST_QUESTIONS: usize = 3;
/// Longest fact, in characters.
pub const MAX_FACT: usize = 300;
/// Longest quote, in characters.
pub const MAX_QUOTE: usize = 600;
/// Longest want label, in characters.
pub const MAX_LABEL: usize = 120;
/// Longest question, in characters.
pub const MAX_QUESTION: usize = 500;
/// Longest skill, in characters.
pub const MAX_SKILL: usize = 60;

/// `text` as the checks compare it: lower case, no whitespace, plain quotes
/// and dashes (the profile's source check, the same comparison).
pub fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200b}' | '\u{00ad}' | '\u{feff}'))
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201b}' | '\u{2032}' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{201f}' | '\u{2033}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether `quote` is part of `text`, word for word (as [`squash`] reads it).
pub fn quote_is_verbatim(text: &str, quote: &str) -> bool {
    let quote = squash(quote);
    !quote.is_empty() && squash(text).contains(&quote)
}

/// A fingerprint of `text` (FNV-1a, 64 bits) that ignores blanks, to notice
/// when a posting changed.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.split_whitespace().flat_map(str::bytes) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

// ---- company facts ----------------------------------------------------------

/// The facts call's answer, as it comes.
#[derive(Debug, Default, Deserialize)]
pub struct RawFacts {
    #[serde(default)]
    facts: Vec<RawFact>,
}

#[derive(Debug, Deserialize)]
struct RawFact {
    #[serde(default)]
    text: String,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    url: String,
}

/// A fact that passed the checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    pub text: String,
    pub quote: String,
    /// The page it was read on, exactly as it was fetched.
    pub url: String,
}

/// The facts the checks keep, and how many they left out. `pages` are the
/// pages that were read, as `(url, text)`.
pub fn judge_facts(raw: RawFacts, pages: &[(String, String)]) -> (Vec<NewFact>, u32) {
    let mut kept: Vec<NewFact> = Vec::new();
    let mut dropped = 0_u32;
    for fact in raw.facts {
        let text = fact.text.trim().to_owned();
        let quote = fact.quote.trim().to_owned();
        let url = fact.url.trim().to_owned();
        let page = pages.iter().find(|(page_url, _)| *page_url == url);
        let fits = !text.is_empty()
            && text.chars().count() <= MAX_FACT
            && quote.chars().count() <= MAX_QUOTE;
        let Some((_, page_text)) = page else {
            dropped += 1;
            continue;
        };
        if !fits || !quote_is_verbatim(page_text, &quote) {
            dropped += 1;
            continue;
        }
        let key = squash(&text);
        if kept.iter().any(|f| squash(&f.text) == key) {
            continue;
        }
        if kept.len() == MOST_FACTS {
            dropped += 1;
            continue;
        }
        kept.push(NewFact { text, quote, url });
    }
    (kept, dropped)
}

// ---- job analysis -----------------------------------------------------------

/// The analysis call's answer, as it comes.
#[derive(Debug, Default, Deserialize)]
pub struct RawAnalysis {
    #[serde(default)]
    wants: Vec<RawWant>,
    #[serde(default)]
    questions: Vec<RawQuestion>,
}

#[derive(Debug, Deserialize)]
struct RawWant {
    #[serde(default)]
    label: String,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    level: String,
}

#[derive(Debug, Deserialize)]
struct RawQuestion {
    #[serde(default)]
    question: String,
    #[serde(default)]
    about: Option<i64>,
    #[serde(default)]
    skill: Option<String>,
}

/// How much the job wants a thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// It must have it.
    Must,
    /// It would be nice.
    Nice,
}

impl Level {
    /// As stored and sent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Must => "must",
            Self::Nice => "nice",
        }
    }

    /// The level stored as `raw`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "must" => Some(Self::Must),
            "nice" => Some(Self::Nice),
            _ => None,
        }
    }
}

/// Something the job wants, with the posting's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub label: String,
    pub quote: String,
    pub level: Level,
}

/// A question where the profile is thin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuestion {
    pub question: String,
    pub about: Option<usize>,
    pub skill: Option<String>,
}

/// What the analysis comes to once the rules have run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analysis {
    pub wants: Vec<Want>,
    pub questions: Vec<NewQuestion>,
    /// Wants left out because their quote is not in the posting.
    pub dropped: u32,
}

/// Run the rules over the model's analysis of `posting`, for `profile`.
pub fn judge_analysis(raw: RawAnalysis, posting: &str, profile: &ProfileDigest) -> Analysis {
    let mut analysis = Analysis::default();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for want in raw.wants {
        let label = want.label.trim().to_owned();
        let quote = want.quote.trim().to_owned();
        let Some(level) = Level::parse(&want.level) else {
            analysis.dropped += 1;
            continue;
        };
        if label.is_empty()
            || label.chars().count() > MAX_LABEL
            || quote.chars().count() > MAX_QUOTE
            || !quote_is_verbatim(posting, &quote)
        {
            analysis.dropped += 1;
            continue;
        }
        if seen.insert(squash(&label), ()).is_some() {
            continue;
        }
        if analysis.wants.len() == MOST_WANTS {
            analysis.dropped += 1;
            continue;
        }
        analysis.wants.push(Want {
            label,
            quote,
            level,
        });
    }

    let mut asked: HashMap<String, ()> = HashMap::new();
    for question in raw.questions {
        let text = question.question.trim().to_owned();
        if text.is_empty() || text.chars().count() > MAX_QUESTION {
            continue;
        }
        let skill = question
            .skill
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty() && s.chars().count() <= MAX_SKILL);
        if skill.as_deref().is_some_and(|s| {
            profile
                .dismissed_skills
                .iter()
                .any(|d| d.eq_ignore_ascii_case(s) || *d == s.to_lowercase())
        }) {
            continue;
        }
        if asked.insert(squash(&text), ()).is_some() {
            continue;
        }
        let about = question
            .about
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| profile.roles.iter().any(|r| r.index == *i));
        if analysis.questions.len() == MOST_QUESTIONS {
            break;
        }
        analysis.questions.push(NewQuestion {
            question: text,
            about,
            skill,
        });
    }
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RoleDigest;
    use serde_json::json;

    fn pages() -> Vec<(String, String)> {
        vec![
            (
                "https://example.test/about".into(),
                "Example Co builds invented ledgers for small shops.\nWe are 40 people in Porto."
                    .into(),
            ),
            (
                "https://example.test/blog/hotwire".into(),
                "In March 2026 we started moving from React to Hotwire.".into(),
            ),
        ]
    }

    fn raw_facts(value: serde_json::Value) -> RawFacts {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_quote_is_the_pages_words_whatever_their_spacing_or_quotes() {
        let text = "We said \u{201c}ledgers\u{201d} \u{2014} for  small\nshops.";
        assert!(quote_is_verbatim(text, "\"ledgers\" - for small shops"));
        assert!(quote_is_verbatim(text, "  FOR SMALL SHOPS "));
        assert!(!quote_is_verbatim(text, "for large shops"));
        assert!(!quote_is_verbatim(text, ""));
        assert!(!quote_is_verbatim(text, "   "));
    }

    #[test]
    fn a_fact_stands_only_on_a_quote_found_in_the_page_it_cites() {
        let (kept, dropped) = judge_facts(
            raw_facts(json!({ "facts": [
                { "text": "Builds ledgers for small shops",
                  "quote": "builds invented ledgers for small shops",
                  "url": "https://example.test/about" },
                { "text": "Is moving from React to Hotwire",
                  "quote": "moving from React to Hotwire",
                  "url": "https://example.test/blog/hotwire" },
                // The quote is on the blog, not on the page it cites.
                { "text": "Is moving from React to Hotwire",
                  "quote": "moving from React to Hotwire",
                  "url": "https://example.test/about" },
                // A page that was never read.
                { "text": "Has 900 staff", "quote": "We are 900 people",
                  "url": "https://news.example/story" },
                // A quote nobody wrote.
                { "text": "Was founded in 1999", "quote": "founded in 1999",
                  "url": "https://example.test/about" },
                // Said twice: kept once, not counted.
                { "text": "Builds ledgers for small shops",
                  "quote": "ledgers for small shops", "url": "https://example.test/about" },
            ]})),
            &pages(),
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].url, "https://example.test/blog/hotwire");
        assert_eq!(dropped, 3);
    }

    #[test]
    fn at_most_twelve_facts_are_kept() {
        let facts: Vec<_> = (0..15)
            .map(|n| {
                json!({ "text": format!("Fact {n}"), "quote": "We are 40 people in Porto",
                        "url": "https://example.test/about" })
            })
            .collect();
        let (kept, dropped) = judge_facts(raw_facts(json!({ "facts": facts })), &pages());
        assert_eq!(kept.len(), MOST_FACTS);
        assert_eq!(dropped, 3);
    }

    fn profile() -> ProfileDigest {
        ProfileDigest {
            roles: vec![
                RoleDigest {
                    index: 0,
                    position: Some("Senior Engineer".into()),
                    ..RoleDigest::default()
                },
                RoleDigest {
                    index: 1,
                    ..RoleDigest::default()
                },
            ],
            skills: vec!["Rust".into()],
            latest: Some(0),
            dismissed_skills: vec!["hotwire".into()],
        }
    }

    const POSTING: &str =
        "Staff Engineer.\nYou will own billing. 5+ years of Ruby required.\nKafka is a plus.";

    fn analysis(value: serde_json::Value) -> Analysis {
        judge_analysis(serde_json::from_value(value).unwrap(), POSTING, &profile())
    }

    #[test]
    fn a_want_must_quote_the_posting() {
        let a = analysis(json!({ "wants": [
            { "label": "Ruby", "quote": "5+ years of Ruby required", "level": "must" },
            { "label": "Kafka", "quote": "kafka is a plus", "level": "nice" },
            { "label": "Go", "quote": "Go experience", "level": "must" },
            { "label": "Odd level", "quote": "own billing", "level": "essential" },
        ], "questions": [] }));
        assert_eq!(a.wants.len(), 2);
        assert_eq!(a.wants[0].level, Level::Must);
        assert_eq!(a.wants[1].label, "Kafka");
        assert_eq!(a.dropped, 2);
    }

    #[test]
    fn at_most_six_wants_and_three_questions_and_never_a_repeat() {
        let wants: Vec<_> = (0..8)
            .map(|n| json!({ "label": format!("Thing {n}"), "quote": "own billing", "level": "must" }))
            .collect();
        let questions: Vec<_> = (0..5)
            .map(|n| json!({ "question": format!("Question {n}?"), "about": 0, "skill": null }))
            .chain([json!({ "question": "Question 0?", "about": 0, "skill": null })])
            .collect();
        let a = analysis(json!({ "wants": wants, "questions": questions }));
        assert_eq!(a.wants.len(), MOST_WANTS);
        assert_eq!(a.dropped, 2);
        assert_eq!(a.questions.len(), MOST_QUESTIONS);
        assert_eq!(a.questions[0].question, "Question 0?");
    }

    #[test]
    fn a_dismissed_skill_is_not_asked_about_and_a_missing_position_becomes_none() {
        let a = analysis(json!({ "wants": [], "questions": [
            { "question": "Have you used Hotwire?", "about": 0, "skill": "Hotwire" },
            { "question": "Have you run Kafka?", "about": 9, "skill": " Kafka " },
            { "question": "Have you led a team?", "about": -1, "skill": null },
            { "question": "   ", "about": 0, "skill": null },
            { "question": "Have you run Kafka?", "about": 1, "skill": "Kafka" },
        ]}));
        assert_eq!(a.questions.len(), 2);
        assert_eq!(a.questions[0].about, None, "9 is not a position");
        assert_eq!(a.questions[0].skill.as_deref(), Some("Kafka"));
        assert_eq!(a.questions[1].about, None);
    }

    #[test]
    fn a_posting_has_a_fingerprint_that_ignores_blanks() {
        assert_eq!(fingerprint("a  b\nc"), fingerprint("a b c"));
        assert_ne!(fingerprint("a b c"), fingerprint("a b d"));
    }
}
