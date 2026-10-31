//! What the model said, and how far it is believed. No model here: every
//! rule is plain code, and what fails a rule is **flagged or downgraded,
//! never dropped**.
//!
//! - A portrait sentence the model calls `known` or `told` must pass the
//!   claim check ([`applicamp_kernel::claims`]) against the sources it cites.
//!   One that does not becomes `assumed`, with the reason that it is not in
//!   the profile in these words.
//! - A change proposed from what the person wrote ("Anything else") carries
//!   their words as its quote. A claim in its value that their words do not
//!   say is a flag naming the claim; a quote that is not in their words is a
//!   flag too. The change stays.

use applicamp_kernel::{
    claims::{Evidence, squash},
    tech,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    goals::{MAX_ITEM, MAX_LIST, MAX_ROLES},
    schema::{Area, MAX_LONG, MAX_SHORT},
};

/// Most sentences a portrait has.
pub const MAX_SENTENCES: usize = 16;
/// Longest sentence, in characters.
pub const MAX_SENTENCE: usize = 600;
/// Most changes one "Anything else" can propose.
pub const MAX_CHANGES: usize = 12;
/// Longest skill, in characters.
pub const MAX_SKILL: usize = 100;

// ---- the portrait -----------------------------------------------------------

/// Where a sentence comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SentenceKind {
    /// The profile says it.
    Known,
    /// The person told Applicamp (a fact they wrote or confirmed).
    Told,
    /// Applicamp assumed it.
    Assumed,
}

/// What the model may cite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A value of the profile or the goals.
    Profile,
    /// A fact the person wrote or confirmed.
    Told,
    /// A fact read on an employer's own site.
    Research,
}

/// One thing the model may cite.
#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    pub text: String,
    /// For research: the page's own words, and the page.
    pub quote: Option<String>,
    pub url: Option<String>,
}

/// A sentence as the model wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawSentence {
    pub text: String,
    pub kind: SentenceKind,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub position: Option<u32>,
}

/// The portrait as the model wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawPortrait {
    pub sentences: Vec<RawSentence>,
}

/// A sentence as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentence {
    pub id: String,
    pub text: String,
    pub kind: SentenceKind,
    /// The ids it cites.
    pub sources: Vec<String>,
    /// For an assumption: why it was assumed.
    pub why: Option<String>,
    /// The check moved it from `known` or `told` to `assumed`.
    pub downgraded: bool,
    /// What it is about.
    pub topic: Area,
    /// The position it is about (its place in `work`), and that position as
    /// it was then, so a list edited since never lends it to another.
    pub position: Option<u32>,
    pub role_key: Option<String>,
    /// The pages it was read on.
    pub urls: Vec<String>,
    /// Told just now, from "Anything else".
    pub new: bool,
}

/// A judged portrait.
#[derive(Debug, Clone)]
pub struct Portrait {
    pub sentences: Vec<Sentence>,
    /// Sentences the check moved to `assumed`.
    pub downgraded: u32,
}

/// Cut `text` to `max` characters.
pub fn cut(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Judge `raw` against `sources`. `role_keys` are the positions' keys, by
/// their place in `work`.
pub fn judge_portrait(raw: RawPortrait, sources: &[Source], role_keys: &[String]) -> Portrait {
    let mut sentences = Vec::new();
    let mut downgraded = 0;
    for item in raw.sentences.into_iter().take(MAX_SENTENCES) {
        let text = cut(&item.text, MAX_SENTENCE);
        if text.is_empty() {
            continue;
        }
        let cited: Vec<&Source> = item
            .sources
            .iter()
            .filter_map(|id| sources.iter().find(|s| &s.id == id))
            .collect();
        let mut kind = item.kind;
        let mut why = Some(cut(&item.why, MAX_SENTENCE)).filter(|w| !w.is_empty());
        let mut moved = false;
        if kind != SentenceKind::Assumed {
            let allowed = |s: &&Source| match kind {
                SentenceKind::Told => s.kind == SourceKind::Told,
                _ => s.kind != SourceKind::Research,
            };
            let usable: Vec<&Source> = cited.iter().copied().filter(allowed).collect();
            let supported = !usable.is_empty()
                && Evidence::of_texts(usable.iter().map(|s| s.text.as_str()))
                    .unsupported(&text)
                    .is_empty();
            if !supported {
                kind = SentenceKind::Assumed;
                why = None;
                moved = true;
                downgraded += 1;
            }
        }
        let position = item
            .position
            .filter(|p| usize::try_from(*p).is_ok_and(|p| p < role_keys.len()));
        sentences.push(Sentence {
            id: format!("s{}", sentences.len() + 1),
            text,
            kind,
            sources: cited.iter().map(|s| s.id.clone()).collect(),
            why: if kind == SentenceKind::Assumed {
                why
            } else {
                None
            },
            downgraded: moved,
            topic: Area::parse(&item.topic).unwrap_or(Area::Context),
            position: if kind == SentenceKind::Assumed {
                position
            } else {
                None
            },
            role_key: if kind == SentenceKind::Assumed {
                position.and_then(|p| role_keys.get(usize::try_from(p).ok()?).cloned())
            } else {
                None
            },
            urls: cited.iter().filter_map(|s| s.url.clone()).collect(),
            new: false,
        });
    }
    Portrait {
        sentences,
        downgraded,
    }
}

// ---- what the person wrote ---------------------------------------------------

/// What a change is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Summary,
    Headline,
    /// One skill.
    Skill,
    Background,
    /// A role they want next.
    GoalRole,
    /// Something they will not work on.
    GoalAvoid,
    /// Something the job must have.
    GoalMustHave,
    /// A place they can work from.
    GoalLocation,
    /// A fact about a named position.
    RoleFact,
}

impl Field {
    /// Whether the profile holds one of them.
    pub const fn single(self) -> bool {
        matches!(self, Self::Summary | Self::Headline | Self::Background)
    }

    /// The most characters a value may have.
    pub const fn max(self) -> usize {
        match self {
            Self::Summary | Self::Background | Self::RoleFact => MAX_LONG,
            Self::Headline => MAX_SHORT,
            Self::Skill => MAX_SKILL,
            _ => MAX_ITEM,
        }
    }
}

/// A change as the model proposed it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawChange {
    pub field: Field,
    pub value: String,
    #[serde(default)]
    pub position: Option<u32>,
    #[serde(default)]
    pub area: String,
    pub quote: String,
    #[serde(default)]
    pub sentence: String,
}

/// The changes as the model proposed them.
#[derive(Debug, Clone, Deserialize)]
pub struct RawChanges {
    pub changes: Vec<RawChange>,
}

/// What is wrong with a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    /// A claim in the value that the person's words do not say.
    Claim,
    /// The quote is not in the person's words.
    Quote,
}

/// A flag on a change: shown, never a reason to drop it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Flag {
    pub kind: FlagKind,
    /// The claim (empty for a quote).
    pub claim: String,
}

/// How the person settled a flagged change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// "It's true": they stand behind it.
    True,
    /// They fixed the value.
    Fixed,
    /// "Use my own words".
    OwnWords,
}

/// A pending change to the profile, from what the person wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub field: Field,
    pub value: String,
    /// For a role fact: the position and what it was.
    pub position: Option<u32>,
    pub role_key: Option<String>,
    pub area: Area,
    /// Their words that support it.
    pub quote: String,
    /// The sentence it adds to the portrait.
    pub sentence: String,
    pub flags: Vec<Flag>,
    pub resolution: Option<Resolution>,
}

impl Change {
    /// Whether a flag still waits for the person.
    pub fn open(&self) -> bool {
        !self.flags.is_empty() && self.resolution.is_none()
    }
}

/// Whether `text` holds `quote` (no case, no spacing, plain quotes).
pub fn says(text: &str, quote: &str) -> bool {
    let quote = squash(quote);
    !quote.is_empty() && squash(text).contains(&quote)
}

/// The flags of `value` against `text` (what the person wrote) and `quote`.
pub fn flags_of(text: &str, value: &str, quote: &str) -> Vec<Flag> {
    let mut flags = Vec::new();
    if !says(text, quote) {
        flags.push(Flag {
            kind: FlagKind::Quote,
            claim: String::new(),
        });
    }
    for claim in Evidence::of_texts([text]).unsupported(value) {
        flags.push(Flag {
            kind: FlagKind::Claim,
            claim,
        });
    }
    flags
}

/// Judge `raw` against `text`, the person's words. `role_keys` are the
/// positions' keys, by their place in `work`.
pub fn judge_changes(raw: RawChanges, text: &str, role_keys: &[String]) -> Vec<Change> {
    let evidence = Evidence::of_texts([text]);
    let mut changes: Vec<Change> = Vec::new();
    for item in raw.changes {
        if changes.len() >= MAX_CHANGES {
            break;
        }
        let value = cut(&item.value, item.field.max());
        if value.is_empty()
            || (item.field.single() && changes.iter().any(|c| c.field == item.field))
        {
            continue;
        }
        if changes
            .iter()
            .any(|c| c.field == item.field && squash(&c.value) == squash(&value))
        {
            continue;
        }
        let quote = cut(&item.quote, MAX_LONG);
        let flags = flags_of(text, &value, &quote);
        // The sentence the portrait gets says only what they wrote; failing
        // that, it is their own words.
        let sentence = cut(&item.sentence, MAX_SENTENCE);
        let sentence = if !sentence.is_empty() && evidence.unsupported(&sentence).is_empty() {
            sentence
        } else if says(text, &quote) {
            cut(&quote, MAX_SENTENCE)
        } else {
            value.clone()
        };
        let position = (item.field == Field::RoleFact)
            .then_some(item.position)
            .flatten()
            .filter(|p| usize::try_from(*p).is_ok_and(|p| p < role_keys.len()));
        changes.push(Change {
            id: format!("c{}", changes.len() + 1),
            field: item.field,
            value,
            position,
            role_key: position.and_then(|p| role_keys.get(usize::try_from(p).ok()?).cloned()),
            area: Area::parse(&item.area).unwrap_or(Area::YourPart),
            quote,
            sentence,
            flags,
            resolution: None,
        });
    }
    changes
}

/// The text "Use my own words" puts in `change`'s field: for the summary,
/// the whole answer; for the rest, the words the change was taken from.
pub fn own_words(change: &Change, text: &str) -> String {
    let words = if change.field == Field::Summary {
        text
    } else if says(text, &change.quote) {
        &change.quote
    } else {
        text
    };
    cut(words, change.field.max())
}

/// Goal lists have room for `n` more.
pub fn goal_room(field: Field, have: usize) -> bool {
    match field {
        Field::GoalRole => have < MAX_ROLES,
        _ => have < MAX_LIST,
    }
}

/// Whether `a` and `b` are the same skill.
pub fn same(field: Field, a: &str, b: &str) -> bool {
    if field == Field::Skill {
        tech::same_skill(a, b)
    } else {
        squash(a) == squash(b)
    }
}
