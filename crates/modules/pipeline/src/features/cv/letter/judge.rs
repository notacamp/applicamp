//! What the model answered, made into a draft: shaped and capped, never
//! believed. What it says is checked afterwards by [`super::check`].

use serde::Deserialize;

use super::{
    super::model::Contact,
    model::{Answer, Content, Paragraph, Sentence},
};
use crate::ports::CvProfile;

/// Longest sentence, in characters.
pub const MAX_SENTENCE: usize = 400;
/// Longest answer to a job question, in characters.
pub const MAX_ANSWER: usize = 1_200;
/// Paragraphs, and sentences in one paragraph.
const MAX_PARAGRAPHS: usize = 6;
const MAX_SENTENCES: usize = 6;
/// The most characters in all the paragraphs.
const MAX_LETTER: usize = 4_000;
/// Sources one sentence may cite.
const MAX_CITED: usize = 8;

/// A sentence (or an answer's text) as the model gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawText {
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

/// A paragraph as the model gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawParagraph {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub sentences: Vec<RawText>,
}

/// An answer as the model gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawAnswer {
    pub question: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

/// The model's answer.
#[derive(Debug, Clone, Deserialize)]
pub struct RawLetter {
    #[serde(default)]
    pub paragraphs: Vec<RawParagraph>,
    #[serde(default)]
    pub answers: Vec<RawAnswer>,
}

fn cited(sources: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in sources {
        let id = id.trim().to_owned();
        if !id.is_empty() && !out.contains(&id) && out.len() < MAX_CITED {
            out.push(id);
        }
    }
    out
}

/// `text` cut into its sentences at a full stop, `!` or `?` followed by a
/// space; a paragraph the model gave without them.
fn split(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()) {
            out.push(std::mem::take(&mut current));
        }
    }
    out.push(current);
    out.into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A normal form of a question to compare the model's copy with the posting's.
fn key(question: &str) -> String {
    question
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The draft the model's answer comes to, with `profile`'s contact word for
/// word. One answer for each of `questions` (the posting's), in order; one the
/// model left out, or gave no text for, has no text. `None` when the letter
/// says nothing.
pub fn judge(raw: RawLetter, questions: &[String], profile: &CvProfile) -> Option<Content> {
    let mut content = Content {
        contact: Contact::from(profile.contact.clone()),
        ..Content::default()
    };
    let mut budget = MAX_LETTER;
    for paragraph in raw.paragraphs.into_iter().take(MAX_PARAGRAPHS) {
        let parts: Vec<RawText> = if paragraph.sentences.is_empty() {
            split(&paragraph.text)
                .into_iter()
                .map(|text| RawText {
                    text,
                    sources: Vec::new(),
                })
                .collect()
        } else {
            paragraph.sentences
        };
        let mut sentences = Vec::new();
        for part in parts.into_iter().take(MAX_SENTENCES) {
            let text: String = part.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text: String = text.chars().take(MAX_SENTENCE).collect();
            let used = text.chars().count();
            if text.is_empty() || used > budget {
                continue;
            }
            budget -= used;
            sentences.push(Sentence {
                id: content.new_id("s"),
                text,
                sources: cited(part.sources),
                edited: false,
            });
        }
        if !sentences.is_empty() {
            let id = content.new_id("p");
            content.paragraphs.push(Paragraph { id, sentences });
        }
    }
    for question in questions {
        let given = raw
            .answers
            .iter()
            .find(|a| key(&a.question) == key(question));
        let text = given
            .map(|a| {
                a.text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(MAX_ANSWER)
                    .collect::<String>()
            })
            .unwrap_or_default();
        // No text, no sources: there is nothing for them to back.
        let sources = if text.is_empty() {
            Vec::new()
        } else {
            given.map(|a| cited(a.sources.clone())).unwrap_or_default()
        };
        let id = content.new_id("q");
        content.answers.push(Answer {
            id,
            question: question.clone(),
            text,
            sources,
            own: false,
        });
    }
    (!content.paragraphs.is_empty()).then_some(content)
}

/// A sentence or answer by the person's hand: at most `most` characters.
pub fn edited(text: &str, most: usize) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= most).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_without_sentences_is_cut_into_them_and_cites_nothing() {
        assert_eq!(
            split("I like it. Do you? Yes! Version 2.5 works."),
            ["I like it.", "Do you?", "Yes!", "Version 2.5 works."]
        );
    }

    #[test]
    fn every_question_gets_an_answer_slot_and_a_missing_one_has_no_text() {
        let raw = RawLetter {
            paragraphs: vec![RawParagraph {
                text: "Hello there.".into(),
                sentences: vec![],
            }],
            answers: vec![RawAnswer {
                question: "WHY do you want this job?".into(),
                text: "Because.".into(),
                sources: vec!["w0.h1".into(), "w0.h1".into()],
            }],
        };
        let questions = vec![
            "Why do you want this job?".to_owned(),
            "What is your notice period?".to_owned(),
        ];
        let content = judge(raw, &questions, &CvProfile::default()).unwrap();
        assert_eq!(content.answers.len(), 2);
        assert_eq!(content.answers[0].text, "Because.");
        assert_eq!(content.answers[0].sources, ["w0.h1"]);
        assert_eq!(content.answers[1].text, "");
        assert!(content.answers[1].sources.is_empty());
    }
}
