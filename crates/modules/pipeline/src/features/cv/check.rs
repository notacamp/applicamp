//! The fact check: no model, the gate for Approve.
//!
//! A line of the CV may say only what the sources it cites say. Its claims
//! are the words `applicamp_kernel::claims` picks out, and the ones that
//! matter (#73) must be in the text of the sources it cites (with the head of
//! their positions, and the company's name for a company fact):
//!
//! - a **number** (an amount, a percentage, a duration, a year), a **tool**
//!   (a known technology or a skill of the profile), an **employer** (a
//!   company the profile names, or the job's), an **acronym**: always;
//! - a **name** (a word with a capital mid-sentence) only when it is not a
//!   word that names no fact (Developer, Stack, a month), nor a place or a
//!   language the profile names, nor, in the summary, a word of the posting
//!   (the job's vocabulary, which the summary may use). A rank or a seniority
//!   (Senior, Lead, Head, Founder) is a claim unless a position's title says
//!   it. The headline may also say what any position's title says. Places,
//!   languages and the posting's words match whole words only ("Cisco" is not
//!   in "San Francisco").
//!
//! And:
//!
//! - a source id the frame does not have counts as no source;
//! - a bullet under a position is backed only by that position's sources and
//!   the skills: another position's is flagged, and the profile's headline,
//!   summary, education and languages do not back it;
//! - a company fact may be cited in the summary only: it says how the
//!   person's real experience fits the company, never what they achieved;
//! - every skill must be one the profile lists (aliases count, as in
//!   `applicamp_kernel::tech`).
//!
//! Each line that fails carries the reasons, and is blocked: Approve stays
//! off until none is.

use std::collections::{BTreeMap, HashSet};

use applicamp_kernel::{
    claims::{Claim, ClaimKind, Evidence, claims, classified},
    tech,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::model::{Contact, Content, Frame, Line, Place, Source, SourceKind};

/// Most claims named for one line.
const MAX_CLAIMS: usize = 3;

/// Words written with a capital that name no fact: what a role, a team or a
/// date is called. Lower case.
const PLAIN: &[&str] = &[
    "mid",
    "intermediate",
    "engineer",
    "engineers",
    "engineering",
    "developer",
    "developers",
    "development",
    "programmer",
    "software",
    "full",
    "stack",
    "fullstack",
    "frontend",
    "front",
    "backend",
    "back",
    "end",
    "web",
    "mobile",
    "cloud",
    "platform",
    "platforms",
    "infrastructure",
    "data",
    "product",
    "products",
    "design",
    "designer",
    "management",
    "architecture",
    "consultant",
    "analyst",
    "scientist",
    "specialist",
    "expert",
    "team",
    "teams",
    "tech",
    "technical",
    "technology",
    "technologies",
    "system",
    "systems",
    "solutions",
    "operations",
    "security",
    "quality",
    "support",
    "assistant",
    "freelance",
    "freelancer",
    "contractor",
    "remote",
    "hybrid",
    "application",
    "applications",
    "service",
    "services",
    "present",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Ranks and seniorities: a claim about the person unless a position's title
/// says it. Lower case.
const RANK: &[&str] = &[
    "senior",
    "junior",
    "lead",
    "principal",
    "staff",
    "head",
    "chief",
    "manager",
    "director",
    "architect",
    "intern",
    "associate",
    "founder",
    "co",
    "owner",
];

/// What a line may say without citing it, and which of its words are names
/// that matter: built once per check from the frame, the person's place and
/// the posting.
#[derive(Debug, Clone)]
pub struct Known {
    /// The posting's title and text: the job's vocabulary.
    posting: Evidence,
    /// The places the profile names: the person's and every position's.
    places: Evidence,
    /// The languages the profile lists.
    languages: Evidence,
    /// Every position's title: what the headline may say.
    titles: Vec<String>,
    /// The words of every position's title, lower case: the ranks a line may
    /// say.
    title_words: HashSet<String>,
    /// The words of employers' names (the profile's and the job's), lower
    /// case: a claim wherever they appear.
    employers: HashSet<String>,
    /// The profile's skills: a tool claim even when no list knows them.
    skills: Vec<String>,
}

impl Known {
    /// What `frame`, the person's `contact` place and the `posting` (its
    /// title and text) allow.
    pub fn new(frame: &Frame, contact: &Contact, posting: &str) -> Self {
        let mut places: Vec<String> = frame
            .positions
            .iter()
            .filter_map(|p| p.location.clone())
            .collect();
        places.extend(contact.location.clone());
        let mut employers: HashSet<String> = HashSet::new();
        for name in frame
            .positions
            .iter()
            .filter_map(|p| p.company.as_deref())
            .chain(std::iter::once(frame.company.as_str()))
        {
            for word in name.split(|c: char| !c.is_alphanumeric()) {
                let word = word.to_lowercase();
                if word.chars().count() >= 2
                    && !PLAIN.contains(&word.as_str())
                    && !RANK.contains(&word.as_str())
                {
                    employers.insert(word);
                }
            }
        }
        let languages: Vec<&str> = frame
            .sources
            .iter()
            .filter(|s| s.kind == SourceKind::Languages)
            .map(|s| s.text.as_str())
            .collect();
        let titles: Vec<String> = frame
            .positions
            .iter()
            .filter_map(|p| p.title.clone())
            .collect();
        let title_words = titles
            .iter()
            .flat_map(|t| t.split(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        Self {
            posting: Evidence::of_texts([posting]),
            places: Evidence::of_texts(places),
            languages: Evidence::of_texts(languages),
            titles,
            title_words,
            employers,
            skills: frame.skills.clone(),
        }
    }

    fn employer(&self, word: &str) -> bool {
        self.employers.contains(&word.to_lowercase())
    }

    fn skill(&self, word: &str) -> bool {
        self.skills
            .iter()
            .any(|s| tech::same_skill(s, word) || s.eq_ignore_ascii_case(word))
    }

    /// Whether `claim`, which the cited sources do not say, may still be
    /// written in a line at `place`.
    fn allows(&self, claim: &Claim, place: Place) -> bool {
        let word = claim.text.as_str();
        if self.employer(word) || self.skill(word) {
            return false;
        }
        let lower = word.to_lowercase();
        if RANK.contains(&lower.as_str()) {
            // A rank is the person's only when a position's title says it.
            return self.title_words.contains(&lower);
        }
        // Whole words: "Cisco" is not in "San Francisco".
        let place_or_language = self.places.says_words(word) || self.languages.says_words(word);
        let posting = place == Place::Summary && self.posting.says_words(word);
        match claim.kind {
            ClaimKind::Number | ClaimKind::Tool => false,
            ClaimKind::Acronym => place_or_language || posting,
            ClaimKind::Name => PLAIN.contains(&lower.as_str()) || place_or_language || posting,
        }
    }
}

/// Why a line cannot be traced to the profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    /// The line says something (`claim`: a number, a tool, a name) that the
    /// sources it cites do not.
    ClaimNotInSources {
        claim: String,
        /// The sources it cites.
        cites: Vec<String>,
    },
    /// The line cites no source the profile has.
    NoSource,
    /// A company fact is cited outside the summary: company facts can't be
    /// the person's achievements.
    CompanyFactOutsideSummary {
        /// The company facts it cites.
        cites: Vec<String>,
    },
    /// A bullet cites what the profile says about another position: the
    /// line belongs to another job.
    OtherPosition {
        /// The other positions' sources it cites.
        cites: Vec<String>,
    },
    /// The skill is not on the profile.
    SkillNotOnProfile { skill: String },
}

/// The reasons of every line that fails, by line id (a skill's too).
pub type Checks = BTreeMap<String, Vec<Reason>>;

/// Whether `source` may back a line at `place`: under a position, only that
/// position's sources and the skills; a company fact only in the summary.
fn backs_at(source: &Source, place: Place) -> bool {
    match place {
        Place::Bullet(own) => source.position == Some(own) || source.kind == SourceKind::Skills,
        Place::Summary => true,
        Place::Headline => source.kind != SourceKind::Company,
    }
}

/// The sources of `line` at `place` that may back it: the ones the frame
/// has, without a company fact outside the summary, and under a position
/// only that position's and the skills.
pub fn usable_sources(line: &Line, place: Place, frame: &Frame) -> Vec<String> {
    line.sources
        .iter()
        .filter(|id| frame.get(id).is_some_and(|s| backs_at(s, place)))
        .cloned()
        .collect()
}

/// The reasons `line` at `place` fails, none when it is traced to the
/// profile.
pub fn check_line(line: &Line, place: Place, frame: &Frame, known: &Known) -> Vec<Reason> {
    let valid: Vec<String> = line
        .sources
        .iter()
        .filter(|id| frame.get(id).is_some())
        .cloned()
        .collect();
    if valid.is_empty() {
        return vec![Reason::NoSource];
    }
    let mut reasons = Vec::new();
    let company: Vec<String> = valid
        .iter()
        .filter(|id| frame.get(id).is_some_and(|s| s.kind == SourceKind::Company))
        .cloned()
        .collect();
    if !company.is_empty() && place != Place::Summary {
        reasons.push(Reason::CompanyFactOutsideSummary { cites: company });
    }
    let elsewhere: Vec<String> = match place {
        Place::Bullet(own) => valid
            .iter()
            .filter(|id| {
                frame
                    .get(id)
                    .and_then(|s| s.position)
                    .is_some_and(|at| at != own)
            })
            .cloned()
            .collect(),
        Place::Headline | Place::Summary => Vec::new(),
    };
    if !elsewhere.is_empty() {
        reasons.push(Reason::OtherPosition { cites: elsewhere });
    }
    // What backs it: the sources it may use there (never another
    // position's, nor under a position the profile-wide ones), and for the
    // headline every position's title.
    let backing: Vec<String> = valid
        .iter()
        .filter(|id| {
            !reasons_cite(&reasons, id) && frame.get(id).is_some_and(|s| backs_at(s, place))
        })
        .cloned()
        .collect();
    if backing.is_empty() && reasons.is_empty() {
        // Under a position, the summary or the education says nothing of it.
        return vec![Reason::NoSource];
    }
    let mut texts = frame.evidence(&backing);
    if place == Place::Headline {
        texts.extend(known.titles.iter().cloned());
    }
    let evidence = Evidence::of_texts(texts);
    let mut seen: Vec<String> = Vec::new();
    for claim in classified(&line.text) {
        if evidence.says(&claim.text) || known.allows(&claim, place) {
            continue;
        }
        if seen.iter().any(|c| c.eq_ignore_ascii_case(&claim.text)) {
            continue;
        }
        seen.push(claim.text.clone());
        if seen.len() > MAX_CLAIMS {
            break;
        }
        reasons.push(Reason::ClaimNotInSources {
            claim: claim.text,
            cites: valid.clone(),
        });
    }
    reasons
}

/// Whether `id` is another position's source named in `reasons`.
fn reasons_cite(reasons: &[Reason], id: &str) -> bool {
    reasons.iter().any(|r| match r {
        Reason::OtherPosition { cites } => cites.iter().any(|c| c == id),
        _ => false,
    })
}

/// The claims of `text`: the words the check looks for in the sources.
pub fn claims_of(text: &str) -> Vec<String> {
    claims(text)
}

/// Whether the profile lists `skill`.
pub fn skill_on_profile(skill: &str, frame: &Frame) -> bool {
    frame.skills.iter().any(|s| tech::same_skill(s, skill))
}

/// Check the whole `content` against `frame`.
pub fn check(content: &Content, frame: &Frame, known: &Known) -> Checks {
    let mut checks = Checks::new();
    for (line, place) in content.placed_lines() {
        let reasons = check_line(line, place, frame, known);
        if !reasons.is_empty() {
            checks.insert(line.id.clone(), reasons);
        }
    }
    for skill in &content.skills {
        if !skill_on_profile(&skill.name, frame) {
            checks.insert(
                skill.id.clone(),
                vec![Reason::SkillNotOnProfile {
                    skill: skill.name.clone(),
                }],
            );
        }
    }
    checks
}

/// Why "Yes, keep it" is not offered for a flagged line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CannotKeep {
    /// It says what the company does, not what the person did.
    Company,
    /// It is what the person did in another job.
    OtherPosition,
    /// It is longer than a fact the profile keeps.
    TooLong,
}

/// Why "Yes, keep it" may not be offered for a line at `place` flagged for
/// `reasons`, if it may not: never for a company fact used as an
/// achievement, nor for a summary whose flagged words only the company's
/// facts say, nor for another job's line.
pub fn cannot_keep(reasons: &[Reason], place: Place, frame: &Frame) -> Option<CannotKeep> {
    if reasons
        .iter()
        .any(|r| matches!(r, Reason::CompanyFactOutsideSummary { .. }))
    {
        return Some(CannotKeep::Company);
    }
    if reasons
        .iter()
        .any(|r| matches!(r, Reason::OtherPosition { .. }))
    {
        return Some(CannotKeep::OtherPosition);
    }
    if place == Place::Summary {
        let claims: Vec<&str> = reasons
            .iter()
            .filter_map(|r| match r {
                Reason::ClaimNotInSources { claim, .. } => Some(claim.as_str()),
                _ => None,
            })
            .collect();
        let company = Evidence::of_texts(
            frame
                .sources
                .iter()
                .filter(|s| s.kind == SourceKind::Company)
                .map(|s| s.text.as_str())
                .chain(std::iter::once(frame.company.as_str())),
        );
        if !claims.is_empty() && claims.iter().all(|c| company.says(c)) {
            return Some(CannotKeep::Company);
        }
    }
    None
}

/// The ids of the lines that fail, in the order they are on the page.
pub fn flagged_in_order(content: &Content, checks: &Checks) -> Vec<String> {
    let mut ids: Vec<String> = content
        .lines()
        .map(|(l, _)| l.id.clone())
        .chain(content.skills.iter().map(|s| s.id.clone()))
        .collect();
    // The page shows the headline, the summary, the roles, then the skills.
    ids.retain(|id| checks.contains_key(id));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        features::cv::model::{self, Contact, Role, SkillItem},
        ports::{CvFact, CvPosition, CvProfile},
    };

    const POSTING: &str = "Senior Full Stack Developer\nRemote across Europe and the Americas. \
                           You will build the API of Example Co's ledgers. Kubernetes a plus.";

    fn frame() -> Frame {
        Frame::build(
            &CvProfile {
                positions: vec![
                    CvPosition {
                        index: 0,
                        title: Some("Senior Engineer".into()),
                        company: Some("Example Org".into()),
                        location: Some("Lisbon, Portugal".into()),
                        highlights: vec![
                            "Rebuilt billing in Rust, cutting invoice runs from 6 hours to 40 minutes"
                                .into(),
                        ],
                        facts: vec![CvFact {
                            id: "11111111-aaaa".into(),
                            text: "Led a team of 4".into(),
                        }],
                        ..CvPosition::default()
                    },
                    CvPosition {
                        index: 1,
                        title: Some("Engineer".into()),
                        company: Some("Sample Labs".into()),
                        highlights: vec!["Built ingestion for 3 partner feeds".into()],
                        ..CvPosition::default()
                    },
                ],
                skills: vec!["Rust".into(), "PostgreSQL".into()],
                latest: Some(0),
                ..CvProfile::default()
            },
            "Example Co",
            &[model::CompanyFact {
                id: "99999999-bbbb".into(),
                text: "Builds ledgers for small shops".into(),
                url: "https://example.test/".into(),
            }],
        )
    }

    fn known(f: &Frame) -> Known {
        Known::new(
            f,
            &Contact {
                location: Some("Porto, Portugal".into()),
                ..Contact::default()
            },
            POSTING,
        )
    }

    fn line(text: &str, sources: &[&str]) -> Line {
        Line {
            id: "l1".into(),
            text: text.into(),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            ..Line::default()
        }
    }

    fn checked(text: &str, sources: &[&str], place: Place) -> Vec<Reason> {
        let f = frame();
        check_line(&line(text, sources), place, &f, &known(&f))
    }

    const W0: Place = Place::Bullet(0);

    #[test]
    fn a_claim_in_a_cited_source_passes() {
        for (text, sources) in [
            (
                "Rebuilt billing in Rust, cutting invoice runs from 6 hours to 40 minutes",
                ["w0.h1"],
            ),
            ("Cut invoice runs to 40 minutes with Rust", ["w0.h1"]),
            ("Led a team of 4 at Example Org", ["w0.f11111111"]),
        ] {
            assert_eq!(checked(text, &sources, W0), [], "{text}");
        }
    }

    #[test]
    fn a_number_that_no_cited_source_says_fails_and_names_the_claim() {
        assert_eq!(
            checked(
                "Cut invoice runs from 6 hours to 20 minutes",
                &["w0.h1"],
                W0
            ),
            [Reason::ClaimNotInSources {
                claim: "20".into(),
                cites: vec!["w0.h1".into()],
            }]
        );
        // A claim another source says does not help a line that does not cite it.
        assert!(matches!(
            checked("Led a team of 4", &["w0.h1"], W0).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "4"
        ));
    }

    #[test]
    fn plain_words_with_a_capital_are_not_claims() {
        // The headline names the role in the job's words, citing the skills.
        assert_eq!(
            checked("Senior Full Stack Developer", &["sk"], Place::Headline),
            []
        );
        assert_eq!(
            checked(
                "Rust Developer and Platform Engineer",
                &["sk"],
                Place::Headline
            ),
            []
        );
        // A month is not a claim; its year is.
        assert_eq!(
            checked("Rebuilt billing in Rust in March", &["w0.h1"], W0),
            []
        );
    }

    #[test]
    fn a_rank_is_a_claim_unless_a_positions_title_says_it() {
        // "Senior" is in a title of the profile (the first position's): it
        // may be said anywhere, even where that position's head is not cited.
        assert_eq!(
            checked(
                "Built ingestion for 3 partner feeds as a Senior",
                &["w1.h1"],
                Place::Bullet(1)
            ),
            []
        );
        assert_eq!(checked("Wrote Rust as a Senior", &["sk"], W0), []);
        // "Lead", "Staff", "Head", "Founder": no title says them, not even
        // when the posting does.
        for (text, rank) in [
            ("Rust Developer and Team Lead", "Lead"),
            ("Rust Staff Engineer", "Staff"),
            ("Rust, Head of Platform", "Head"),
            ("Rust engineer and Founder", "Founder"),
        ] {
            let reasons = checked(text, &["sk"], Place::Headline);
            assert!(
                reasons
                    .iter()
                    .any(|r| matches!(r, Reason::ClaimNotInSources { claim, .. } if claim == rank)),
                "{text}: {reasons:?}"
            );
        }
        let f = frame();
        let staffed = Known::new(&f, &Contact::default(), "Staff Engineer");
        assert!(
            !check_line(
                &line("Rust engineer at Staff level", &["w0.h1"]),
                Place::Summary,
                &f,
                &staffed,
            )
            .is_empty(),
            "the posting's rank is not the person's"
        );
    }

    #[test]
    fn places_languages_and_the_postings_words_match_whole_words() {
        let f = frame();
        let known = Known::new(
            &f,
            &Contact {
                location: Some("San Francisco, California".into()),
                ..Contact::default()
            },
            "We build for Francisco's teams across Americas",
        );
        let check = |text: &str, place| check_line(&line(text, &["w0.h1"]), place, &f, &known);
        assert_eq!(check("Rebuilt billing in Rust in Francisco", W0), []);
        assert!(matches!(
            check("Rebuilt billing in Rust for Cisco", W0).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Cisco"
        ));
        // Not a word of the posting either, in the summary.
        assert!(matches!(
            check("Rust engineer for Cisco", Place::Summary).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Cisco"
        ));
        assert!(!check("Rust engineer for the Americ teams", Place::Summary).is_empty());
    }

    #[test]
    fn a_place_the_profile_names_passes_anywhere_and_another_does_not() {
        assert_eq!(
            checked("Rust engineer based in Lisbon", &["sk"], Place::Summary),
            []
        );
        assert_eq!(
            checked("Rebuilt billing in Rust from Porto", &["w0.h1"], W0),
            []
        );
        assert!(matches!(
            checked("Rebuilt billing in Rust from Berlin", &["w0.h1"], W0).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Berlin"
        ));
    }

    #[test]
    fn the_summary_may_use_the_jobs_words_but_never_its_numbers_tools_or_employers() {
        // Europe and the Americas, and the API: the posting's words.
        assert_eq!(
            checked(
                "Rust engineer for teams across Europe and the Americas, building an API",
                &["w0.h1"],
                Place::Summary
            ),
            []
        );
        // Not in a bullet: there the words must come from the profile.
        assert!(!checked("Rebuilt billing in Rust across Europe", &["w0.h1"], W0).is_empty());
        // A tool the posting names is not the person's.
        assert!(matches!(
            checked("Rust and Kubernetes engineer", &["sk"], Place::Summary).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Kubernetes"
        ));
    }

    #[test]
    fn a_tool_or_an_employer_nobody_cited_fails() {
        assert!(matches!(
            checked("Rebuilt billing on Kubernetes", &["w0.h1"], W0).as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Kubernetes"
        ));
        // Sample Labs is the person's other employer: not this line's.
        let reasons = checked("Rebuilt billing in Rust for Sample Labs", &["w0.h1"], W0);
        assert!(
            reasons
                .iter()
                .any(|r| matches!(r, Reason::ClaimNotInSources { claim, .. } if claim == "Sample")),
            "{reasons:?}"
        );
        // An employer nobody named.
        assert!(!checked("Rebuilt billing at Invented Corp", &["w0.h1"], W0).is_empty());
    }

    #[test]
    fn a_bullet_may_cite_only_its_own_position() {
        let reasons = checked("Built ingestion for 3 partner feeds", &["w1.h1"], W0);
        assert_eq!(
            reasons.first(),
            Some(&Reason::OtherPosition {
                cites: vec!["w1.h1".into()]
            }),
            "{reasons:?}"
        );
        // The same line under its own position passes.
        assert_eq!(
            checked(
                "Built ingestion for 3 partner feeds",
                &["w1.h1"],
                Place::Bullet(1)
            ),
            []
        );
        // The skills may back any position's line.
        assert_eq!(checked("Wrote Rust", &["sk"], W0), []);
    }

    #[test]
    fn the_headline_may_say_any_positions_title() {
        assert_eq!(
            checked("Senior Engineer, Rust", &["sk"], Place::Headline),
            []
        );
    }

    #[test]
    fn an_unknown_source_counts_as_no_source() {
        assert_eq!(
            checked("Rebuilt billing in Rust", &["w9.h1"], W0),
            [Reason::NoSource]
        );
        assert_eq!(
            checked("Rebuilt billing in Rust", &[], W0),
            [Reason::NoSource]
        );
        // A known one next to it still has to say it.
        assert_eq!(
            checked("Rebuilt billing in Rust", &["w9.h1", "w0.h1"], W0),
            []
        );
    }

    #[test]
    fn a_company_fact_is_for_the_summary_only() {
        let text = "Builds ledgers for small shops";
        let cited = ["c99999999"];
        assert_eq!(checked(text, &cited, Place::Summary), []);
        assert_eq!(
            checked(text, &cited, W0),
            [Reason::CompanyFactOutsideSummary {
                cites: vec!["c99999999".into()]
            }]
        );
        // The company's name is the company's: a summary may say it.
        assert_eq!(
            checked(
                "Rust engineer for what Example Co builds",
                &["w0.h1", "c99999999"],
                Place::Summary
            ),
            []
        );
        let f = frame();
        assert_eq!(
            cannot_keep(&checked(text, &cited, W0), W0, &f),
            Some(CannotKeep::Company)
        );
        assert_eq!(
            cannot_keep(&checked("Led a team of 12", &["w0.h1"], W0), W0, &f),
            None
        );
    }

    #[test]
    fn what_cannot_be_kept_says_why() {
        let f = frame();
        // Another job's line.
        let other = checked("Built ingestion for 3 partner feeds", &["w1.h1"], W0);
        assert_eq!(cannot_keep(&other, W0, &f), Some(CannotKeep::OtherPosition));
        // A summary whose flagged words only the company's facts say.
        let with_company = Frame::build(
            &CvProfile {
                positions: vec![CvPosition {
                    index: 0,
                    title: Some("Engineer".into()),
                    highlights: vec!["Rebuilt billing in Rust".into()],
                    ..CvPosition::default()
                }],
                skills: vec!["Rust".into()],
                latest: Some(0),
                ..CvProfile::default()
            },
            "Example Co",
            &[model::CompanyFact {
                id: "99999999-bbbb".into(),
                text: "Runs payroll for 300 Bakeries".into(),
                url: "https://example.test/".into(),
            }],
        );
        let summary = check_line(
            &line("Rust engineer who ran payroll for 300 Bakeries", &["w0.h1"]),
            Place::Summary,
            &with_company,
            &known(&with_company),
        );
        assert_eq!(summary.len(), 2, "{summary:?}");
        assert_eq!(
            cannot_keep(&summary, Place::Summary, &with_company),
            Some(CannotKeep::Company)
        );
        // A number the person may have: theirs to keep.
        let number = checked(
            "Rust engineer who cut runs to 20 minutes",
            &["w0.h1"],
            Place::Summary,
        );
        assert_eq!(cannot_keep(&number, Place::Summary, &f), None);
    }

    #[test]
    fn a_bullet_is_backed_by_its_own_position_and_the_skills_only() {
        let f = Frame::build(
            &CvProfile {
                summary: Some("Rust engineer in billing at Example Org".into()),
                positions: vec![CvPosition {
                    index: 0,
                    title: Some("Engineer".into()),
                    company: Some("Example Org".into()),
                    highlights: vec!["Rebuilt billing".into()],
                    ..CvPosition::default()
                }],
                skills: vec!["Rust".into()],
                latest: Some(0),
                ..CvProfile::default()
            },
            "Example Co",
            &[],
        );
        let known = Known::new(&f, &Contact::default(), "");
        // The summary does not back a bullet, so a bullet citing only it
        // cites nothing it may use.
        let from_summary = line("Rebuilt billing in Rust", &["sum"]);
        assert_eq!(usable_sources(&from_summary, W0, &f), Vec::<String>::new());
        assert_eq!(
            check_line(&from_summary, W0, &f, &known),
            [Reason::NoSource]
        );
        // Its words are not evidence for a bullet beside its own source.
        assert!(matches!(
            check_line(&line("Rebuilt billing in Rust", &["w0.h1", "sum"]), W0, &f, &known)
                .as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "Rust"
        ));
        assert_eq!(
            check_line(
                &line("Rebuilt billing in Rust", &["w0.h1", "sk"]),
                W0,
                &f,
                &known
            ),
            []
        );
        // In the summary, it backs a line.
        assert_eq!(check_line(&from_summary, Place::Summary, &f, &known), []);
    }

    #[test]
    fn the_usable_sources_leave_out_what_cannot_back_the_line() {
        let f = frame();
        let mixed = line("x", &["w0.h1", "w1.h1", "c99999999", "sk", "nope"]);
        assert_eq!(usable_sources(&mixed, W0, &f), ["w0.h1", "sk"]);
        assert_eq!(
            usable_sources(&mixed, Place::Summary, &f),
            ["w0.h1", "w1.h1", "c99999999", "sk"]
        );
    }

    #[test]
    fn a_skill_off_the_profile_is_flagged_and_aliases_pass() {
        let f = frame();
        let content = Content {
            contact: Contact::default(),
            experience: vec![Role {
                position: 0,
                bullets: vec![],
            }],
            skills: vec![
                SkillItem {
                    id: "k1".into(),
                    name: "Postgres".into(),
                },
                SkillItem {
                    id: "k2".into(),
                    name: "Kubernetes".into(),
                },
            ],
            ..Content::default()
        };
        let checks = check(&content, &f, &known(&f));
        assert!(!checks.contains_key("k1"), "an alias of a listed skill");
        assert_eq!(
            checks.get("k2"),
            Some(&vec![Reason::SkillNotOnProfile {
                skill: "Kubernetes".into()
            }])
        );
        assert_eq!(flagged_in_order(&content, &checks), ["k2"]);
    }
}
