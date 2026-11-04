//! What the model said about the LinkedIn profile, and how far it is
//! believed. No model here: every rule is plain code, and what fails a rule
//! is **flagged, never dropped**.
//!
//! - A proposal's text may say only what the sources it cites say: every
//!   claim ([`applicamp_kernel::claims`]) not in them is a `claim` flag.
//! - A buzzword ("passionate", "results-driven", "guru"… in the four
//!   languages the app speaks) is a `buzzword` flag unless a cited source
//!   says it.
//! - A flagged proposal stays on the page and cannot be kept until it is
//!   edited to pass, or dropped.

use applicamp_kernel::{
    claims::{Evidence, squash},
    tech,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::prompt::is_linkedin_source;
use crate::features::about::judge::{Source, SourceKind, cut};

/// Most findings kept.
pub const MAX_FINDINGS: usize = 24;
/// Most proposals kept.
pub const MAX_PROPOSALS: usize = 16;
/// Most skills proposed.
pub const MAX_SKILLS: usize = 8;
/// Longest overall read, in characters.
pub const MAX_OVERALL: usize = 1_200;
/// Longest "why", in characters.
pub const MAX_WHY: usize = 300;

/// A part of a LinkedIn profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedInPart {
    Headline,
    /// The About text.
    Summary,
    /// One position, by its index.
    Position,
    Skills,
    Education,
    Languages,
    Certifications,
    Projects,
}

impl LinkedInPart {
    /// Whether a proposal may be made for it.
    const fn takes_proposals(self) -> bool {
        matches!(
            self,
            Self::Headline | Self::Summary | Self::Position | Self::Skills
        )
    }
}

/// How a part does for the goals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedInState {
    /// Fit for the goals as it is.
    Good,
    /// There, but it does not say what the goals need.
    CouldBeBetter,
    /// Empty on LinkedIn while the sources say something for it.
    Missing,
}

/// What a proposal changes on LinkedIn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedInField {
    /// One line, LinkedIn's limit of 220 characters.
    Headline,
    /// The About text, LinkedIn's limit of 2,600 characters.
    Summary,
    /// A position's description, LinkedIn's limit of 2,000 characters.
    PositionDescription,
    /// One skill to add.
    Skill,
}

impl LinkedInField {
    /// The most characters its text may have (LinkedIn's own limits).
    pub const fn max(self) -> usize {
        match self {
            Self::Headline => 220,
            Self::Summary => 2_600,
            Self::PositionDescription => 2_000,
            Self::Skill => 100,
        }
    }

    /// The part it belongs to.
    const fn part(self) -> LinkedInPart {
        match self {
            Self::Headline => LinkedInPart::Headline,
            Self::Summary => LinkedInPart::Summary,
            Self::PositionDescription => LinkedInPart::Position,
            Self::Skill => LinkedInPart::Skills,
        }
    }
}

/// What is wrong with a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkedInFlagKind {
    /// A claim in the text that no cited source says.
    Claim,
    /// A buzzword no cited source says.
    Buzzword,
}

/// A flag on a proposal: shown, never a reason to drop it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedInFlag {
    pub kind: LinkedInFlagKind,
    /// The claim, or the buzzword.
    pub claim: String,
}

/// Where a proposal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Waiting for Keep or Drop.
    Ready,
    /// Kept: on the list of things to change on LinkedIn.
    Kept,
    /// Dropped.
    Dropped,
    /// Changed on LinkedIn.
    Done,
}

/// One source a proposal rests on, as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedInSource {
    pub id: String,
    /// `profile`, `told` or `linkedin`.
    pub kind: String,
    pub text: String,
}

impl LinkedInSource {
    fn of(source: &Source) -> Self {
        let kind = if is_linkedin_source(&source.id) {
            "linkedin"
        } else {
            match source.kind {
                SourceKind::Told => "told",
                _ => "profile",
            }
        };
        Self {
            id: source.id.clone(),
            kind: kind.to_owned(),
            text: source.text.clone(),
        }
    }
}

/// A finding as the model wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawFinding {
    pub part: LinkedInPart,
    #[serde(default)]
    pub position: Option<u32>,
    pub state: LinkedInState,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub proposals: Vec<RawProposal>,
}

/// A proposal as the model wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawProposal {
    pub field: LinkedInField,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub why: String,
}

/// The review as the model wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawReview {
    #[serde(default)]
    pub overall: String,
    #[serde(default)]
    pub findings: Vec<RawFinding>,
}

/// A judged finding: one part, its state, why, and its proposals' ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedInFinding {
    pub id: String,
    pub part: LinkedInPart,
    /// For a position: its index in the LinkedIn profile.
    pub position: Option<u32>,
    pub state: LinkedInState,
    pub why: String,
    /// The ids of its proposals.
    pub proposals: Vec<String>,
}

/// A judged proposal, with what the person did with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedInProposal {
    pub id: String,
    pub field: LinkedInField,
    /// For a position's description: its index in the LinkedIn profile.
    pub position: Option<u32>,
    /// The text to paste on LinkedIn (a skill: its name).
    pub text: String,
    /// What it does for the goals.
    pub why: String,
    /// What it rests on.
    pub sources: Vec<LinkedInSource>,
    pub flags: Vec<LinkedInFlag>,
    pub status: ProposalStatus,
    /// The person rewrote it.
    pub edited: bool,
}

/// A judged review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub overall: String,
    pub findings: Vec<LinkedInFinding>,
    pub proposals: Vec<LinkedInProposal>,
}

/// Words that say nothing a recruiter can check, in the four languages the
/// app speaks: lower case; a phrase is matched without spaces, a word whole.
const BUZZWORDS: &[&str] = &[
    // en
    "passionate",
    "results-driven",
    "results driven",
    "proven track record",
    "track record",
    "guru",
    "ninja",
    "rockstar",
    "rock star",
    "expert",
    "go-to",
    "best",
    "world-class",
    "world class",
    "thought leader",
    "visionary",
    "synergy",
    "self-starter",
    "detail-oriented",
    "team player",
    "seasoned",
    "exceptional",
    // pt
    "apaixonado",
    "apaixonada",
    "orientado a resultados",
    "orientada a resultados",
    "especialista",
    "melhor",
    "excecional",
    "excepcional",
    "visionário",
    "visionária",
    // es
    "apasionado",
    "apasionada",
    "orientado a resultados",
    "orientada a resultados",
    "experto",
    "experta",
    "gurú",
    "mejor",
    "excepcional",
    "visionario",
    "visionaria",
    // fr
    "passionné",
    "passionnée",
    "axé sur les résultats",
    "axée sur les résultats",
    "expert",
    "experte",
    "gourou",
    "meilleur",
    "meilleure",
    "exceptionnel",
    "exceptionnelle",
    "visionnaire",
];

/// The words of `text`, lower case: runs of letters and digits.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `text` says `word` (a phrase without its spaces, a word whole).
fn says_word(text: &str, text_words: &[String], word: &str) -> bool {
    if word.contains([' ', '-']) {
        squash(text).contains(&squash(word))
    } else {
        text_words.iter().any(|w| w == word)
    }
}

/// The buzzwords of `text` that no `evidence` text says.
pub fn buzzwords<'a>(text: &str, evidence: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let text_words = words(text);
    let evidence: Vec<&str> = evidence.into_iter().collect();
    let evidence_text = evidence.join("\n");
    let evidence_words = words(&evidence_text);
    let mut found: Vec<String> = Vec::new();
    for word in BUZZWORDS {
        if says_word(text, &text_words, word)
            && !says_word(&evidence_text, &evidence_words, word)
            && !found.iter().any(|f| f == word)
        {
            found.push((*word).to_owned());
        }
    }
    found
}

/// The flags of `text` against `sources`: every claim and every buzzword
/// the sources do not say. With no source, every claim is a flag.
pub fn flags_of(text: &str, sources: &[LinkedInSource]) -> Vec<LinkedInFlag> {
    let texts: Vec<&str> = sources.iter().map(|s| s.text.as_str()).collect();
    let evidence = Evidence::of_texts(texts.iter().copied());
    let mut flags: Vec<LinkedInFlag> = evidence
        .unsupported(text)
        .into_iter()
        .map(|claim| LinkedInFlag {
            kind: LinkedInFlagKind::Claim,
            claim,
        })
        .collect();
    flags.extend(
        buzzwords(text, texts.iter().copied())
            .into_iter()
            .map(|word| LinkedInFlag {
                kind: LinkedInFlagKind::Buzzword,
                claim: word,
            }),
    );
    flags
}

/// Judge `raw` against `sources`; `positions` is how many positions the
/// LinkedIn profile has, `known_skills` its skills.
pub fn judge(
    raw: RawReview,
    sources: &[Source],
    positions: usize,
    known_skills: &[String],
) -> Review {
    let mut findings: Vec<LinkedInFinding> = Vec::new();
    let mut proposals: Vec<LinkedInProposal> = Vec::new();
    let mut skills = 0;
    for item in raw.findings {
        if findings.len() >= MAX_FINDINGS {
            break;
        }
        let position = (item.part == LinkedInPart::Position)
            .then_some(item.position)
            .flatten()
            .filter(|p| usize::try_from(*p).is_ok_and(|p| p < positions));
        if item.part == LinkedInPart::Position && position.is_none() {
            continue;
        }
        if findings
            .iter()
            .any(|f| f.part == item.part && f.position == position)
        {
            continue;
        }
        let mut ids = Vec::new();
        if item.part.takes_proposals() && item.state != LinkedInState::Good {
            for proposal in item.proposals {
                if proposals.len() >= MAX_PROPOSALS || proposal.field.part() != item.part {
                    continue;
                }
                let text = cut(&proposal.text, proposal.field.max());
                if text.is_empty() {
                    continue;
                }
                let twice = proposal.field != LinkedInField::Skill
                    && proposals
                        .iter()
                        .any(|p| ids.contains(&p.id) && p.field == proposal.field);
                if twice {
                    continue;
                }
                if proposal.field == LinkedInField::Skill {
                    let known = known_skills.iter().any(|k| tech::same_skill(k, &text))
                        || proposals.iter().any(|p| {
                            p.field == LinkedInField::Skill && tech::same_skill(&p.text, &text)
                        });
                    if known || skills >= MAX_SKILLS {
                        continue;
                    }
                    skills += 1;
                }
                let cited: Vec<LinkedInSource> = proposal
                    .sources
                    .iter()
                    .filter_map(|id| sources.iter().find(|s| &s.id == id))
                    .map(LinkedInSource::of)
                    .collect();
                let flags = flags_of(&text, &cited);
                let id = format!("p{}", proposals.len() + 1);
                ids.push(id.clone());
                proposals.push(LinkedInProposal {
                    id,
                    field: proposal.field,
                    position: (proposal.field == LinkedInField::PositionDescription)
                        .then_some(position)
                        .flatten(),
                    text,
                    why: cut(&proposal.why, MAX_WHY),
                    sources: cited,
                    flags,
                    status: ProposalStatus::Ready,
                    edited: false,
                });
            }
        }
        findings.push(LinkedInFinding {
            id: format!("f{}", findings.len() + 1),
            part: item.part,
            position,
            state: item.state,
            why: cut(&item.why, MAX_WHY),
            proposals: ids,
        });
    }
    Review {
        overall: cut(&raw.overall, MAX_OVERALL),
        findings,
        proposals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, text: &str) -> Source {
        Source {
            id: id.to_owned(),
            kind: SourceKind::Profile,
            text: text.to_owned(),
            quote: None,
            url: None,
        }
    }

    fn sources() -> Vec<Source> {
        vec![
            source("p.headline", "Backend engineer"),
            source(
                "w0",
                "Senior Engineer at Example Org. Runs the billing platform in Rust.",
            ),
            source("l.headline", "Backend engineer · Rust and Postgres"),
        ]
    }

    fn raw(findings: Vec<RawFinding>) -> RawReview {
        RawReview {
            overall: "Your headline says what you do, not what you want next.".to_owned(),
            findings,
        }
    }

    fn finding(
        part: LinkedInPart,
        position: Option<u32>,
        proposals: Vec<RawProposal>,
    ) -> RawFinding {
        RawFinding {
            part,
            position,
            state: LinkedInState::CouldBeBetter,
            why: "It does not name the role you want.".to_owned(),
            proposals,
        }
    }

    fn proposal(field: LinkedInField, text: &str, sources: &[&str]) -> RawProposal {
        RawProposal {
            field,
            text: text.to_owned(),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            why: "Names the role.".to_owned(),
        }
    }

    #[test]
    fn a_claim_no_source_says_is_flagged_not_dropped() {
        let review = judge(
            raw(vec![finding(
                LinkedInPart::Headline,
                None,
                vec![proposal(
                    LinkedInField::Headline,
                    "Senior Engineer running billing on Kubernetes",
                    &["w0"],
                )],
            )]),
            &sources(),
            1,
            &[],
        );
        assert_eq!(review.proposals.len(), 1);
        let flags = &review.proposals[0].flags;
        assert_eq!(flags.len(), 1, "{flags:?}");
        assert_eq!(flags[0].kind, LinkedInFlagKind::Claim);
        assert_eq!(flags[0].claim, "Kubernetes");
        assert_eq!(review.findings[0].proposals, ["p1"]);
    }

    #[test]
    fn what_the_sources_say_passes() {
        let review = judge(
            raw(vec![finding(
                LinkedInPart::Headline,
                None,
                vec![proposal(
                    LinkedInField::Headline,
                    "Senior Engineer · billing platform in Rust",
                    &["w0", "l.headline"],
                )],
            )]),
            &sources(),
            1,
            &[],
        );
        assert!(
            review.proposals[0].flags.is_empty(),
            "{:?}",
            review.proposals[0].flags
        );
        assert_eq!(review.proposals[0].sources.len(), 2);
        assert_eq!(review.proposals[0].sources[1].kind, "linkedin");
    }

    #[test]
    fn a_buzzword_is_flagged_unless_a_source_says_it() {
        let flags = flags_of(
            "A passionate engineer, the best in Rust.",
            &[LinkedInSource {
                id: "w0".to_owned(),
                kind: "profile".to_owned(),
                text: "Runs the billing platform in Rust.".to_owned(),
            }],
        );
        let words: Vec<&str> = flags.iter().map(|f| f.claim.as_str()).collect();
        assert_eq!(words, ["passionate", "best"], "{flags:?}");
        assert!(flags.iter().all(|f| f.kind == LinkedInFlagKind::Buzzword));
        let cleared = flags_of(
            "Passionate about Rust.",
            &[LinkedInSource {
                id: "w0".to_owned(),
                kind: "told".to_owned(),
                text: "I am passionate about Rust".to_owned(),
            }],
        );
        assert!(cleared.is_empty(), "{cleared:?}");
        // "best" is a whole word: "bestseller" is not it.
        assert!(buzzwords("A bestseller on laptops", Vec::<&str>::new()).is_empty());
    }

    #[test]
    fn no_source_passes_only_with_no_claim() {
        assert!(flags_of("Backend engineer who keeps systems calm.", &[]).is_empty());
        let flags = flags_of("Backend engineer at Example Org.", &[]);
        assert_eq!(flags.len(), 2, "{flags:?}");
    }

    #[test]
    fn positions_skills_and_counts_are_held_to_the_profile() {
        let known = vec!["Rust".to_owned()];
        let review = judge(
            raw(vec![
                // A position the LinkedIn profile does not have.
                finding(LinkedInPart::Position, Some(3), vec![]),
                finding(
                    LinkedInPart::Position,
                    Some(0),
                    vec![
                        proposal(
                            LinkedInField::PositionDescription,
                            "Runs the billing platform.",
                            &["w0"],
                        ),
                        // A second description for the same position.
                        proposal(LinkedInField::PositionDescription, "Runs billing.", &["w0"]),
                        // A headline under a position.
                        proposal(LinkedInField::Headline, "Engineer", &["w0"]),
                    ],
                ),
                finding(
                    LinkedInPart::Skills,
                    None,
                    vec![
                        proposal(LinkedInField::Skill, "rust", &["w0"]),
                        proposal(LinkedInField::Skill, "Postgres", &["l.headline"]),
                        proposal(LinkedInField::Skill, "PostgreSQL", &["l.headline"]),
                    ],
                ),
                // Education takes no proposals.
                finding(
                    LinkedInPart::Education,
                    None,
                    vec![proposal(LinkedInField::Summary, "Studied.", &[])],
                ),
                // The same part twice.
                finding(LinkedInPart::Skills, None, vec![]),
            ]),
            &sources(),
            1,
            &known,
        );
        let parts: Vec<(LinkedInPart, Option<u32>)> = review
            .findings
            .iter()
            .map(|f| (f.part, f.position))
            .collect();
        assert_eq!(
            parts,
            [
                (LinkedInPart::Position, Some(0)),
                (LinkedInPart::Skills, None),
                (LinkedInPart::Education, None),
            ]
        );
        let texts: Vec<&str> = review.proposals.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Runs the billing platform.", "Postgres"],
            "{texts:?}"
        );
        assert_eq!(review.proposals[0].position, Some(0));
        assert!(review.findings[2].proposals.is_empty());
    }

    #[test]
    fn a_good_part_gets_no_proposal() {
        let mut item = finding(
            LinkedInPart::Summary,
            None,
            vec![proposal(
                LinkedInField::Summary,
                "Backend engineer.",
                &["p.headline"],
            )],
        );
        item.state = LinkedInState::Good;
        let review = judge(raw(vec![item]), &sources(), 0, &[]);
        assert!(review.proposals.is_empty());
        assert_eq!(review.findings[0].state, LinkedInState::Good);
    }

    #[test]
    fn texts_are_cut_to_linkedins_limits() {
        let long = "x".repeat(300);
        let review = judge(
            raw(vec![finding(
                LinkedInPart::Headline,
                None,
                vec![proposal(LinkedInField::Headline, &long, &[])],
            )]),
            &sources(),
            0,
            &[],
        );
        assert_eq!(review.proposals[0].text.chars().count(), 220);
    }
}
