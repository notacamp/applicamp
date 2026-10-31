//! The claim check: a line of text may say only what its sources say.
//!
//! Moved here from the profile module (#37) because both the profile's draft
//! check and a tailored CV's fact check (pipeline) use it, and modules never
//! import each other. It is about text, not about any domain.
//!
//! First written for the draft source check: a draft made from the person's
//! answer may say only what the answer or the profile already says.
//!
//! **The rule.** From each draft line we take every *claim*: a word that
//! holds a digit (a number, a percentage, an amount, a version), names a
//! technology ([`crate::tech`]), is written in capitals (an
//! acronym), or starts with a capital in the middle of a sentence (a company,
//! a product, a place). Each claim must appear in the evidence, which is the
//! answer and every value of the profile; a line with a claim that does not
//! is dropped and counted. The comparison is the import source check's: no
//! case, no whitespace, plain quotes and dashes ([`squash`]); numbers match
//! whole numbers only (`4` is not in `40`); words of up to three letters
//! match whole words (`Go` is not in `Google`). A hyphenated or slashed word
//! is checked part by part.
//!
//! The check is strict on purpose: a true line it cannot confirm (a number
//! the person spelled out as a word) is left out and the person is told,
//! never the other way round.

use std::collections::HashSet;

use serde_json::Value;

use crate::tech;

/// `text` as the check compares it: lower case, no whitespace, plain quotes
/// and dashes.
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

/// Capitalised words that name nothing: no claim.
const COMMON: &[&str] = &[
    "i", "a", "an", "the", "and", "or", "but", "to", "in", "on", "at", "for", "with", "by", "of",
    "from", "as", "after", "before", "while", "when", "my", "our", "we", "it", "its", "this",
    "that", "these", "those", "into", "over", "under", "across", "through",
];

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

/// What the person has said: the answer and their profile.
#[derive(Debug, Clone)]
pub struct Evidence {
    squashed: String,
    words: HashSet<String>,
}

impl Evidence {
    /// The evidence of `answer` and the values of `profile`.
    pub fn of(answer: &str, profile: &Value) -> Self {
        let mut parts = vec![answer.to_owned()];
        leaves(profile, &mut parts);
        let all = parts.join("\n");
        Self {
            squashed: squash(&all),
            words: words(&all).collect(),
        }
    }

    /// The evidence of `texts`, each as written.
    pub fn of_texts<I, S>(texts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let all = texts
            .into_iter()
            .map(|t| t.as_ref().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            squashed: squash(&all),
            words: words(&all).collect(),
        }
    }

    /// Whether the evidence says `claim` (a word).
    fn says(&self, claim: &str) -> bool {
        let squashed = squash(claim);
        if squashed.is_empty() {
            return true;
        }
        let has_digit = claim.chars().any(|c| c.is_ascii_digit());
        let has_letter = claim.chars().any(char::is_alphabetic);
        if has_digit {
            if has_letter {
                // `40min`, `S3`, `2M`: as written.
                return self.squashed.contains(&squashed);
            }
            let digits: String = claim.chars().filter(char::is_ascii_digit).collect();
            return self.words.contains(&digits);
        }
        if squashed.chars().count() <= 3 {
            return self.words.contains(&squashed);
        }
        self.squashed.contains(&squashed)
    }

    /// Whether the evidence contains `text` as written (a URL).
    pub fn contains(&self, text: &str) -> bool {
        self.squashed.contains(&squash(text))
    }

    /// The claims in `line` the evidence does not say.
    pub fn unsupported(&self, line: &str) -> Vec<String> {
        claims(line)
            .into_iter()
            .filter(|claim| !self.says(claim))
            .collect()
    }

    /// `lines` without the ones that claim something unsaid, and how many
    /// were dropped.
    pub fn keep(&self, lines: Vec<String>) -> (Vec<String>, u32) {
        let total = lines.len();
        let kept: Vec<String> = lines
            .into_iter()
            .filter(|line| self.unsupported(line).is_empty())
            .collect();
        let dropped = u32::try_from(total - kept.len()).unwrap_or(u32::MAX);
        (kept, dropped)
    }
}

fn is_acronym(part: &str) -> bool {
    part.chars().filter(|c| c.is_alphabetic()).count() >= 2
        && part
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
}

/// The words of `line` that claim something.
pub fn claims(line: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut sentence_start = true;
    for raw in line.split_whitespace() {
        let token = raw
            .trim_matches(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#'))
            .trim_end_matches('.');
        let ends_sentence = raw.ends_with(['.', '!', '?', ':', ';']);
        if token.is_empty() {
            sentence_start = sentence_start || ends_sentence;
            continue;
        }
        let tech_name = tech::is_name(token);
        for (at, part) in token
            .split(['-', '/'])
            .filter(|p| !p.is_empty())
            .enumerate()
        {
            let digit = part.chars().any(|c| c.is_ascii_digit());
            let capital = part.chars().next().is_some_and(char::is_uppercase)
                && !(sentence_start && at == 0)
                && !COMMON.contains(&part.to_lowercase().as_str());
            if digit || capital || is_acronym(part) || tech::is_name(part) {
                found.push(part.to_owned());
            }
        }
        if tech_name && !found.iter().any(|c| c.eq_ignore_ascii_case(token)) {
            found.push(token.to_owned());
        }
        sentence_start = ends_sentence;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn evidence() -> Evidence {
        Evidence::of(
            "Rebuilt billing in Rust at Example Org. Invoice runs went from 6 hours to 40 minutes, \
             team of 4.",
            &json!({ "work": [{ "name": "Example Org", "position": "Senior Engineer" }],
                     "skills": [{ "name": "PostgreSQL" }] }),
        )
    }

    #[test]
    fn what_the_person_wrote_passes() {
        let e = evidence();
        for line in [
            "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
            "Led a team of 4 at Example Org",
            "Cut invoice runs by hours",
            "Worked with PostgreSQL",
        ] {
            assert!(
                e.unsupported(line).is_empty(),
                "{line}: {:?}",
                e.unsupported(line)
            );
        }
    }

    #[test]
    fn a_number_a_company_or_a_tool_nobody_wrote_fails() {
        let e = evidence();
        for (line, claim) in [
            ("Cut invoice runs from 6 hours to 20 minutes", "20"),
            ("Rebuilt billing at Invented Corp", "Invented"),
            ("Rebuilt billing in Kubernetes", "Kubernetes"),
            ("Saved 30% of cost", "30"),
            ("Led a team of 12", "12"),
            ("Handled GDPR audits", "GDPR"),
        ] {
            let found = e.unsupported(line);
            assert!(
                found.iter().any(|c| c == claim),
                "{line}: expected {claim} in {found:?}"
            );
        }
    }

    #[test]
    fn numbers_and_short_words_match_whole() {
        let e = Evidence::of("A team of 40 at Google", &json!({}));
        assert!(!e.unsupported("Led 4 people").is_empty(), "4 is not 40");
        assert!(
            !e.unsupported("Wrote Go services").is_empty(),
            "Go is not Google"
        );
        assert!(e.unsupported("Led 40 people at Google").is_empty());
    }

    #[test]
    fn sentence_starts_are_not_proper_nouns() {
        let e = Evidence::of("shipped the ledger", &json!({}));
        assert!(e.unsupported("Shipped the ledger").is_empty());
        assert!(e.unsupported("Shipped the ledger. Cut costs").is_empty());
    }

    #[test]
    fn squashing_ignores_case_spacing_and_typography() {
        assert_eq!(squash(" Rock’n  Roll\u{2013}Co \n"), "rock'nroll-co");
    }

    #[test]
    fn evidence_of_texts_checks_against_those_texts_only() {
        let e = Evidence::of_texts(["Cut runs from 6 hours to 40 minutes", "Used Rust"]);
        assert!(e.unsupported("Cut runs to 40 minutes with Rust").is_empty());
        assert_eq!(e.unsupported("Cut runs by 30%"), ["30"]);
    }

    #[test]
    fn keep_drops_and_counts_lines() {
        let e = evidence();
        let (kept, dropped) = e.keep(vec![
            "Rebuilt billing in Rust".to_owned(),
            "Won an award at Acme".to_owned(),
        ]);
        assert_eq!(kept, ["Rebuilt billing in Rust"]);
        assert_eq!(dropped, 1);
    }
}
