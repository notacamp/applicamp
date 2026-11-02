//! What a cover letter is made of, as it is stored: paragraphs of sentences,
//! each sentence citing the sources (a [`super::super::model::Frame`]'s ids)
//! it comes from, and the answers to the job's own questions.
//!
//! Like the CV, the contact details are the server's: they are filled in
//! from the profile when the draft is made and never shown to the model.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::super::model::Contact;

/// How long a letter is meant to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LetterSize {
    /// About 180 words: recruiters skim.
    #[default]
    Short,
    /// About 300 words.
    Standard,
}

impl LetterSize {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Standard => "standard",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "short" => Some(Self::Short),
            "standard" => Some(Self::Standard),
            _ => None,
        }
    }

    /// About how many words the model is asked for.
    pub const fn words(self) -> u32 {
        match self {
            Self::Short => 180,
            Self::Standard => 300,
        }
    }
}

/// A sentence of the letter: its words and the sources it says they come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentence {
    pub id: String,
    pub text: String,
    pub sources: Vec<String>,
    /// The person wrote this text.
    #[serde(default)]
    pub edited: bool,
}

/// A paragraph: sentences, shown one after the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    pub id: String,
    pub sentences: Vec<Sentence>,
}

impl Paragraph {
    /// Its text, as the PDF shows it.
    pub fn text(&self) -> String {
        self.sentences
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The answer to one of the job's own questions. Not in the PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub id: String,
    pub question: String,
    /// Empty when the profile cannot support one: the person answers.
    pub text: String,
    pub sources: Vec<String>,
    /// The person wrote it (so it is not checked against the profile).
    #[serde(default)]
    pub own: bool,
}

/// The draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub contact: Contact,
    pub paragraphs: Vec<Paragraph>,
    pub answers: Vec<Answer>,
    /// The next number of an id (`s<n>`, `p<n>`, `q<n>`).
    pub next_id: u32,
}

impl Content {
    /// A new id with `prefix`.
    pub fn new_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    /// Every sentence, in the order they are on the page.
    pub fn sentences(&self) -> impl Iterator<Item = &Sentence> {
        self.paragraphs.iter().flat_map(|p| p.sentences.iter())
    }

    /// The sentence `id`, with the paragraph and place it is at.
    pub fn locate(&self, id: &str) -> Option<(usize, usize)> {
        self.paragraphs.iter().enumerate().find_map(|(p, para)| {
            para.sentences
                .iter()
                .position(|s| s.id == id)
                .map(|s| (p, s))
        })
    }

    /// How many words the letter's paragraphs have.
    pub fn words(&self) -> usize {
        self.sentences()
            .map(|s| s.text.split_whitespace().count())
            .sum()
    }
}
