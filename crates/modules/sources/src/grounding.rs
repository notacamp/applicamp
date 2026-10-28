//! The reason source check: a match reason may say only what the person's
//! profile and goals say.
//!
//! The scoring model once told someone whose profile listed no skills
//! that a job matched their "Full-stack TypeScript". A `why` reason is
//! the model's claim about the person, so it is checked against what the
//! model was shown (the profile summary, the goals, where they live).
//!
//! **The rule.** From a reason we take every *claim*: a word that names a
//! technology (a known name, [`applicamp_kernel::tech`]), starts with a
//! capital (a framework, a company, a title, a place), or holds a digit (a
//! number of years, a year, a version). Ordinary capitalised words
//! ([`COMMON`]) are no claim. Each claim must appear in the profile or
//! goals text; a reason with a claim that does not is dropped. A word
//! with a hyphen or slash is checked part by part. Numbers match whole
//! numbers only (`5` is not in `2015`); `60k` also matches `60000`.
//!
//! The comparison is the import source check's: no case, no whitespace,
//! plain quotes and dashes ([`squash`]); words of up to three letters
//! must match as whole words (`Go` is not in `Google`).
//!
//! The model's gaps are not checked: they come from the posting.

use std::collections::HashSet;

use applicamp_kernel::tech;
use serde_json::Value;

/// Capitalised words that name nothing: no claim about the person.
const COMMON: &[&str] = &[
    "a",
    "an",
    "the",
    "this",
    "that",
    "these",
    "those",
    "it",
    "its",
    "their",
    "your",
    "you",
    "they",
    "he",
    "she",
    "his",
    "her",
    "my",
    "our",
    "we",
    "and",
    "or",
    "but",
    "not",
    "no",
    "yes",
    "as",
    "at",
    "by",
    "for",
    "from",
    "in",
    "into",
    "of",
    "on",
    "to",
    "with",
    "without",
    "within",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "has",
    "have",
    "had",
    "does",
    "do",
    "can",
    "could",
    "would",
    "will",
    "may",
    "might",
    "should",
    "matches",
    "match",
    "matching",
    "matched",
    "fits",
    "fit",
    "fitting",
    "aligns",
    "align",
    "aligned",
    "suits",
    "strong",
    "good",
    "great",
    "clear",
    "close",
    "direct",
    "solid",
    "excellent",
    "perfect",
    "role",
    "roles",
    "job",
    "position",
    "company",
    "team",
    "work",
    "working",
    "works",
    "remote",
    "hybrid",
    "onsite",
    "on-site",
    "full-time",
    "part-time",
    "experience",
    "experienced",
    "background",
    "skills",
    "skill",
    "profile",
    "goals",
    "goal",
    "target",
    "targets",
    "seniority",
    "level",
    "years",
    "year",
    "months",
    "preference",
    "preferences",
    "preferred",
    "location",
    "located",
    "based",
    "open",
    "english",
    "salary",
    "pay",
    "paid",
    "above",
    "below",
    "meets",
    "meet",
    "covers",
    "cover",
    "offers",
    "offer",
    "wants",
    "want",
    "wanted",
    "asks",
    "ask",
    "requires",
    "require",
    "needs",
    "need",
    "looking",
    "seeking",
    "seeks",
    "within",
    "across",
    "also",
    "both",
    "all",
    "any",
    "more",
    "most",
    "less",
    "than",
    "then",
    "so",
    "if",
    "when",
    "where",
    "while",
    "which",
    "who",
    "what",
    "how",
    "why",
    "other",
    "same",
    "such",
    "very",
    "well",
    "high",
    "senior",
    "junior",
    "mid",
    "lead",
    "staff",
    "principal",
    "engineer",
    "engineering",
    "developer",
    "development",
    "manager",
    "management",
    "designer",
    "analyst",
    "backend",
    "frontend",
    "full-stack",
    "fullstack",
    "back-end",
    "front-end",
    "software",
    "product",
    "technical",
    "tech",
    "startup",
    "scale-up",
    "mission",
    "impact",
    "growth",
    "stage",
    "early",
    "late",
    "europe",
    "european",
    "eu",
    "timezone",
    "time",
    "zone",
    "hours",
    "contract",
    "permanent",
    "employment",
    "ownership",
    "autonomy",
    "avoid",
    "avoids",
    "must",
    "have",
    "haves",
    "nice",
    "plus",
    "bonus",
    "domain",
    "industry",
    "sector",
    "market",
    "customers",
    "users",
    "global",
    "worldwide",
    "anywhere",
    "country",
    "city",
    "relocation",
    "visa",
    "language",
    "languages",
    "spoken",
    "fluent",
    "native",
    "professional",
    "degree",
    "education",
    "years'",
    "leadership",
    "mentoring",
    "mentor",
    "collaboration",
    "communication",
    "ownership",
];

/// `text` as the check compares it: lower case, no whitespace, plain
/// quotes and dashes.
fn squash(text: &str) -> String {
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

/// Every string and number a JSON value holds (never its keys).
fn leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
        Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
        Value::Null | Value::Bool(_) => {}
    }
}

/// Runs of letters and digits in `text`, lower case.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// What the person's profile and goals say.
#[derive(Debug, Clone)]
pub struct Source {
    squashed: String,
    words: HashSet<String>,
}

impl Source {
    /// The text of `texts`: the profile summary, the goals, where they
    /// live (JSON values count by their strings and numbers).
    pub fn of(values: &[&Value], extra: &[&str]) -> Self {
        let mut parts: Vec<String> = Vec::new();
        for value in values {
            leaves(value, &mut parts);
        }
        parts.extend(extra.iter().map(|s| (*s).to_owned()));
        let all = parts.join("\n");
        Self {
            squashed: squash(&all),
            words: words(&all).collect(),
        }
    }

    /// Whether the source says `claim` (a word).
    fn says(&self, claim: &str) -> bool {
        let digits: String = claim.chars().filter(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            // A number, a year, a version: whole numbers only.
            let thousands = claim
                .to_lowercase()
                .contains('k')
                .then(|| format!("{digits}000"));
            let versioned = claim.chars().any(char::is_alphabetic) && !claim.ends_with(['k', 'K']);
            return self.words.contains(&digits)
                || thousands.is_some_and(|t| self.words.contains(&t))
                || (versioned && self.squashed.contains(&squash(claim)));
        }
        let squashed = squash(claim);
        if squashed.is_empty() {
            return true;
        }
        if squashed.chars().count() <= 3 {
            return self.words.contains(&squashed);
        }
        self.squashed.contains(&squashed)
    }

    /// Whether the source lists a technology, by name.
    pub fn names_a_technology(&self) -> bool {
        self.words.iter().any(|w| tech::is_name(w))
    }

    /// The claims in `reason` the source does not say.
    pub fn unsupported(&self, reason: &str) -> Vec<String> {
        claims(reason)
            .into_iter()
            .filter(|claim| !self.says(claim))
            .collect()
    }

    /// `reasons` without the ones that claim something the source does
    /// not say, and how many were dropped.
    pub fn keep(&self, reasons: Vec<String>) -> (Vec<String>, usize) {
        let total = reasons.len();
        let kept: Vec<String> = reasons
            .into_iter()
            .filter(|reason| self.unsupported(reason).is_empty())
            .collect();
        let dropped = total - kept.len();
        (kept, dropped)
    }
}

/// The words of `reason` that claim something about the person.
pub fn claims(reason: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (position, raw) in reason.split_whitespace().enumerate() {
        let token = raw.trim_matches(|c: char| {
            !(c.is_alphanumeric() || c == '+' || c == '#' || c == '.' || c == '-' || c == '/')
        });
        // Possessives and plurals of a name: "Rails'", "TypeScript's".
        let token = token.trim_end_matches("'s").trim_end_matches('.');
        if token.is_empty() {
            continue;
        }
        if tech::is_name(token) {
            found.push(token.to_owned());
            continue;
        }
        for (inner, part) in token
            .split(['/', '-'])
            .filter(|p| !p.is_empty())
            .enumerate()
        {
            let capitalised = (position > 0 || inner > 0)
                && part.chars().next().is_some_and(char::is_uppercase)
                && !COMMON.contains(&part.to_lowercase().as_str());
            if tech::is_name(part) || part.chars().any(|c| c.is_ascii_digit()) || capitalised {
                found.push(part.to_owned());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn source(profile: &Value) -> Source {
        Source::of(
            &[profile, &json!({ "roles": ["Senior backend engineer"] })],
            &["Lisbon"],
        )
    }

    #[test]
    fn a_technology_the_profile_never_names_is_a_claim_it_fails() {
        let nothing = source(&json!({ "headline": "Engineer", "skills": [] }));
        assert_eq!(
            nothing.unsupported("Full-stack TypeScript matches profile"),
            ["TypeScript"]
        );
        assert!(!nothing.names_a_technology());
    }

    #[test]
    fn what_the_profile_says_is_kept() {
        let ruby = source(&json!({ "skills": ["Ruby", "Rails"], "years_working": 12 }));
        assert!(ruby.unsupported("Rails experience matches").is_empty());
        assert!(
            ruby.unsupported("Strong Ruby background, 12 years")
                .is_empty()
        );
        assert!(
            ruby.unsupported("Remote role in Lisbon fits your goals")
                .is_empty()
        );
        assert!(
            ruby.unsupported("Senior backend role matches your target")
                .is_empty()
        );
        assert!(ruby.names_a_technology());
    }

    #[test]
    fn numbers_companies_and_places_must_be_in_the_text_too() {
        let ruby = source(&json!({ "skills": ["Ruby"], "years_working": 12,
            "positions": [{ "company": "Example Org", "title": "Staff Engineer" }] }));
        assert!(!ruby.unsupported("15 years of Ruby").is_empty());
        assert!(ruby.unsupported("12+ years of Ruby").is_empty());
        assert!(!ruby.unsupported("Led teams at Invented Corp").is_empty());
        assert!(ruby.unsupported("Staff Engineer at Example Org").is_empty());
        assert!(!ruby.unsupported("Based in Berlin").is_empty());
        // Whole numbers only; short words whole.
        assert!(!ruby.unsupported("2 years of Ruby").is_empty());
        let google = source(&json!({ "positions": [{ "company": "Google" }] }));
        assert!(!google.unsupported("Skilled in Go").is_empty());
    }

    #[test]
    fn dropping_counts_what_it_dropped() {
        let ruby = source(&json!({ "skills": ["Ruby", "Rails"] }));
        let (kept, dropped) = ruby.keep(vec![
            "Rails experience matches".into(),
            "Deep TypeScript expertise".into(),
        ]);
        assert_eq!(kept, ["Rails experience matches"]);
        assert_eq!(dropped, 1);
    }
}
