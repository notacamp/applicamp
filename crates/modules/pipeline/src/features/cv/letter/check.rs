//! The letter's fact check: no model, the gate for Approve.
//!
//! The CV's rules ([`super::super::check`]) with the letter's differences:
//!
//! - every claim of a sentence (a number, a tool, a name) must be in the
//!   sources it cites, as on the CV;
//! - **company facts are the letter's exception.** Any paragraph may cite one
//!   to say why the company appeals, but a sentence that cites *only* company
//!   facts can't claim anything about the person ("I led…" is flagged): what
//!   the company does is not what the person did;
//! - a sentence that cites nothing is a "no claims" line (a greeting, an offer
//!   to talk) and passes if it claims nothing;
//! - **superlatives are flagged** ("best", "go-to", "expert"…, in the four
//!   languages) unless a cited source says the word;
//! - an **answer to one of the job's questions** the profile can't support
//!   comes back with no text and is flagged: the person answers it. An answer
//!   with text must cite its sources and passes the same check. One the
//!   person wrote is theirs and is not checked.
//!
//! Each failing sentence or answer is blocked, with the reasons, and Approve
//! stays off until none is.

use std::collections::BTreeMap;

use applicamp_kernel::claims::{Evidence, claims};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    super::model::{Frame, SourceKind},
    model::{Answer, Content, Sentence},
};

/// Most claims named for one sentence.
const MAX_CLAIMS: usize = 3;

/// Why a sentence or an answer can't be traced to the person's profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LetterReason {
    /// It says something (`claim`: a number, a tool, a name) that the sources
    /// it cites do not.
    ClaimNotInSources { claim: String, cites: Vec<String> },
    /// It claims something and cites no source the profile has.
    NoSource,
    /// It cites only facts about the company yet says something about the
    /// person.
    CompanyOnlyAboutYou { cites: Vec<String> },
    /// It praises ("go-to", "best") in words no cited source says.
    Superlative { word: String },
    /// The profile and goals do not say: the person answers it.
    NotInProfile,
}

/// The reasons of every sentence and answer that fails, by id.
pub type Checks = BTreeMap<String, Vec<LetterReason>>;

/// Praise a letter may not make unless a source says it, in en, pt, es, fr.
const SUPERLATIVES: &[&str] = &[
    "best",
    "top",
    "go-to",
    "expert",
    "experts",
    "leading",
    "world-class",
    "unmatched",
    "unrivalled",
    "unrivaled",
    "greatest",
    "finest",
    "outstanding",
    "exceptional",
    "guru",
    "ninja",
    "rockstar",
    "melhor",
    "melhores",
    "líder",
    "especialista",
    "excecional",
    "excepcional",
    "insuperável",
    "referência",
    "mejor",
    "mejores",
    "experto",
    "experta",
    "insuperable",
    "meilleur",
    "meilleure",
    "meilleurs",
    "meilleures",
    "leader",
    "exceptionnel",
    "inégalé",
    "incontournable",
];

/// Words that make a sentence about the speaker.
const FIRST_PERSON: &[&str] = &[
    "i", "i'm", "i've", "i'd", "i'll", "my", "me", "myself", "mine", "eu", "meu", "minha", "meus",
    "minhas", "mim", "fui", "sou", "tenho", "yo", "mi", "mis", "soy", "tengo", "je", "mon", "mes",
    "moi", "suis",
];

/// Ways to say what draws the person to the company: about the person's
/// interest, not what they did.
const APPEAL: &[&str] = &[
    "admire",
    "i like",
    "i'd like",
    "i would like",
    "i love",
    "i want",
    "i'm drawn",
    "i am drawn",
    "i'm excited",
    "i am excited",
    "i'm interested",
    "i am interested",
    "i follow",
    "appeals to me",
    "interests me",
    "draws me",
    "admiro",
    "gosto",
    "gostaria",
    "quero",
    "interessa-me",
    "atrai-me",
    "entusiasma-me",
    "me gusta",
    "me gustaría",
    "quiero",
    "me interesa",
    "me atrae",
    "j'admire",
    "j'aime",
    "j'aimerais",
    "je veux",
    "je souhaite",
    "m'intéresse",
    "m'attire",
];

/// The lower-case words of `text`; apostrophes and hyphens inside a word stay.
fn tokens(text: &str) -> Vec<String> {
    text.replace(['\u{2019}', '\u{2018}'], "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .map(|w| w.trim_matches(['\'', '-']).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The praise in `text`, each word once.
fn superlatives_in(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in tokens(text) {
        if SUPERLATIVES.contains(&word.as_str()) && !found.contains(&word) {
            found.push(word);
        }
    }
    found
}

/// Whether `text` speaks of the person: a first-person word, and not only to
/// say what draws them to the company.
pub fn about_the_person(text: &str) -> bool {
    let words = tokens(text);
    let first = words
        .iter()
        .any(|w| FIRST_PERSON.contains(&w.as_str()) || w.starts_with("j'"));
    if !first {
        return false;
    }
    let joined = words.join(" ");
    !APPEAL.iter().any(|phrase| joined.contains(phrase))
}

/// The reasons `text` fails when it cites `cited`; none when it can be
/// traced.
pub fn check_text(text: &str, cited: &[String], frame: &Frame) -> Vec<LetterReason> {
    let valid: Vec<String> = cited
        .iter()
        .filter(|id| frame.get(id).is_some())
        .cloned()
        .collect();
    let mut reasons = Vec::new();
    if valid.is_empty() {
        if !claims(text).is_empty() {
            reasons.push(LetterReason::NoSource);
        }
    } else {
        let only_company = valid
            .iter()
            .all(|id| frame.get(id).is_some_and(|s| s.kind == SourceKind::Company));
        if only_company && about_the_person(text) {
            reasons.push(LetterReason::CompanyOnlyAboutYou {
                cites: valid.clone(),
            });
        }
        let evidence = Evidence::of_texts(frame.evidence(&valid));
        let mut seen: Vec<String> = Vec::new();
        for claim in evidence.unsupported(text) {
            if seen.iter().any(|c| c.eq_ignore_ascii_case(&claim)) {
                continue;
            }
            seen.push(claim.clone());
            if seen.len() > MAX_CLAIMS {
                break;
            }
            reasons.push(LetterReason::ClaimNotInSources {
                claim,
                cites: valid.clone(),
            });
        }
    }
    let said = tokens(&frame.evidence(&valid).join(" "));
    for word in superlatives_in(text) {
        if !said.contains(&word) {
            reasons.push(LetterReason::Superlative { word });
        }
    }
    reasons
}

/// The reasons `sentence` fails.
pub fn check_sentence(sentence: &Sentence, frame: &Frame) -> Vec<LetterReason> {
    check_text(&sentence.text, &sentence.sources, frame)
}

/// The reasons `answer` fails.
pub fn check_answer(answer: &Answer, frame: &Frame) -> Vec<LetterReason> {
    if answer.own {
        return Vec::new();
    }
    if answer.text.trim().is_empty() {
        return vec![LetterReason::NotInProfile];
    }
    if !answer.sources.iter().any(|id| frame.get(id).is_some()) {
        return vec![LetterReason::NoSource];
    }
    check_text(&answer.text, &answer.sources, frame)
}

/// Check the whole `content` against `frame`.
pub fn check(content: &Content, frame: &Frame) -> Checks {
    let mut checks = Checks::new();
    for sentence in content.sentences() {
        let reasons = check_sentence(sentence, frame);
        if !reasons.is_empty() {
            checks.insert(sentence.id.clone(), reasons);
        }
    }
    for answer in &content.answers {
        let reasons = check_answer(answer, frame);
        if !reasons.is_empty() {
            checks.insert(answer.id.clone(), reasons);
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        features::cv::model::CompanyFact,
        ports::{CvFact, CvPosition, CvProfile},
    };

    fn frame() -> Frame {
        Frame::build(
            &CvProfile {
                positions: vec![CvPosition {
                    index: 0,
                    title: Some("Senior Engineer".into()),
                    company: Some("Example Org".into()),
                    highlights: vec![
                        "Led the Rails 5 to 7 upgrade over 7 months".into(),
                        "Known as the go-to person for billing problems".into(),
                    ],
                    facts: vec![CvFact {
                        id: "11111111-aaaa".into(),
                        text: "Led a team of 4".into(),
                    }],
                    ..CvPosition::default()
                }],
                skills: vec!["Rails".into()],
                latest: Some(0),
                ..CvProfile::default()
            },
            "Example Co",
            &[CompanyFact {
                id: "99999999-bbbb".into(),
                text: "Example Co helps 40 small businesses send their invoices".into(),
                url: "https://example.test/about".into(),
            }],
        )
    }

    fn sentence(text: &str, sources: &[&str]) -> Sentence {
        Sentence {
            id: "s1".into(),
            text: text.into(),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            edited: false,
        }
    }

    #[test]
    fn a_sentence_the_sources_say_passes_and_one_they_do_not_names_the_claim() {
        let f = frame();
        assert_eq!(
            check_sentence(
                &sentence("I led the Rails 5 to 7 upgrade over 7 months.", &["w0.h1"]),
                &f
            ),
            []
        );
        let reasons = check_sentence(
            &sentence("I led the Rails 5 to 7 upgrade over 9 months.", &["w0.h1"]),
            &f,
        );
        assert!(matches!(
            reasons.as_slice(),
            [LetterReason::ClaimNotInSources { claim, .. }] if claim == "9"
        ));
    }

    #[test]
    fn a_sentence_that_cites_only_company_facts_cannot_claim_anything_about_the_person() {
        let f = frame();
        let cited = ["c99999999"];
        // The company's own line, and the person's interest in it, are fine.
        assert_eq!(
            check_sentence(
                &sentence(
                    "Example Co helps 40 small businesses send their invoices.",
                    &cited
                ),
                &f
            ),
            []
        );
        assert_eq!(
            check_sentence(
                &sentence(
                    "I admire how Example Co helps small businesses send invoices.",
                    &cited
                ),
                &f
            ),
            []
        );
        // What the person did is not what the company does.
        let reasons = check_sentence(
            &sentence("I led the invoicing work at Example Co.", &cited),
            &f,
        );
        assert_eq!(
            reasons,
            [LetterReason::CompanyOnlyAboutYou {
                cites: vec!["c99999999".into()]
            }]
        );
        // Next to a profile source it may say why the company appeals.
        assert_eq!(
            check_sentence(
                &sentence(
                    "I led the Rails 5 to 7 upgrade over 7 months, work Example Co would use.",
                    &["w0.h1", "c99999999"]
                ),
                &f
            ),
            []
        );
    }

    #[test]
    fn praise_is_flagged_unless_a_cited_source_says_it() {
        let f = frame();
        let reasons = check_sentence(&sentence("I am the best engineer you could hire.", &[]), &f);
        assert_eq!(
            reasons,
            [LetterReason::Superlative {
                word: "best".into()
            }]
        );
        // The same for each language.
        for text in [
            "Sou a melhor opção para a equipa.",
            "Soy un experto en facturación.",
            "Je suis le meilleur choix.",
        ] {
            assert!(
                matches!(
                    check_sentence(&sentence(text, &[]), &f).as_slice(),
                    [LetterReason::Superlative { .. }]
                ),
                "{text}"
            );
        }
        // Without a source, "go-to" is flagged; the profile says it, so it passes.
        assert_eq!(
            check_sentence(&sentence("I'm the go-to person for billing.", &[]), &f),
            [LetterReason::Superlative {
                word: "go-to".into()
            }]
        );
        assert_eq!(
            check_sentence(
                &sentence("I'm the go-to person for billing.", &["w0.h2"]),
                &f
            ),
            []
        );
    }

    #[test]
    fn a_sentence_that_cites_nothing_is_a_no_claims_line_unless_it_claims_something() {
        let f = frame();
        assert_eq!(
            check_sentence(
                &sentence(
                    "I'd like to bring that to your team and I'm free to talk.",
                    &[]
                ),
                &f
            ),
            []
        );
        assert_eq!(
            check_sentence(&sentence("I cut costs by 30% at Acme.", &[]), &f),
            [LetterReason::NoSource]
        );
    }

    #[test]
    fn an_answer_the_profile_cannot_support_has_no_text_and_is_flagged_but_one_the_person_wrote_is_theirs()
     {
        let f = frame();
        let mut answer = Answer {
            id: "q2".into(),
            question: "What is your notice period?".into(),
            text: String::new(),
            sources: Vec::new(),
            own: false,
        };
        assert_eq!(check_answer(&answer, &f), [LetterReason::NotInProfile]);
        answer.text = "Two weeks.".into();
        assert_eq!(check_answer(&answer, &f), [LetterReason::NoSource]);
        answer.own = true;
        assert_eq!(check_answer(&answer, &f), []);
        let supported = Answer {
            text: "I led the Rails 5 to 7 upgrade over 7 months.".into(),
            sources: vec!["w0.h1".into()],
            own: false,
            ..answer
        };
        assert_eq!(check_answer(&supported, &f), []);
    }

    #[test]
    fn the_first_person_is_found_in_every_language() {
        for text in [
            "I led it",
            "Eu liderei a equipa",
            "Yo lideré el equipo",
            "J'ai dirigé l'équipe",
        ] {
            assert!(about_the_person(text), "{text}");
        }
        assert!(!about_the_person("Example Co builds ledgers"));
    }
}
