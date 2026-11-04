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
//!   word that names no fact (Senior, Developer, Stack, a month), nor a place
//!   or a language the profile names, nor, in the summary, a word of the
//!   posting (the job's vocabulary, which the summary may use). The headline
//!   may also say what any position's title says.
//!
//! And:
//!
//! - a source id the frame does not have counts as no source;
//! - a bullet under a position may cite only that position's sources (and
//!   the profile-wide ones): another position's is flagged;
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

use super::model::{Contact, Content, Frame, Line, Place, SourceKind};

/// Most claims named for one line.
const MAX_CLAIMS: usize = 3;

/// Words written with a capital that name no fact: what a role, a team or a
/// date is called. Lower case.
const PLAIN: &[&str] = &[
    "senior",
    "junior",
    "mid",
    "intermediate",
    "lead",
    "principal",
    "staff",
    "head",
    "chief",
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
    "manager",
    "management",
    "director",
    "architect",
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
    "intern",
    "associate",
    "assistant",
    "founder",
    "co",
    "owner",
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
                if word.chars().count() >= 2 && !PLAIN.contains(&word.as_str()) {
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
        Self {
            posting: Evidence::of_texts([posting]),
            places: Evidence::of_texts(places),
            languages: Evidence::of_texts(languages),
            titles: frame
                .positions
                .iter()
                .filter_map(|p| p.title.clone())
                .collect(),
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
        let place_or_language = self.places.says(word) || self.languages.says(word);
        match claim.kind {
            ClaimKind::Number | ClaimKind::Tool => false,
            ClaimKind::Acronym => {
                place_or_language || (place == Place::Summary && self.posting.says(word))
            }
            ClaimKind::Name => {
                PLAIN.contains(&word.to_lowercase().as_str())
                    || place_or_language
                    || (place == Place::Summary && self.posting.says(word))
            }
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

/// The sources of `line` at `place` that may back it: the ones the frame
/// has, without a company fact outside the summary and without another
/// position's source under a position.
pub fn usable_sources(line: &Line, place: Place, frame: &Frame) -> Vec<String> {
    line.sources
        .iter()
        .filter(|id| {
            frame.get(id).is_some_and(|s| {
                let company = s.kind == SourceKind::Company && place != Place::Summary;
                let elsewhere = matches!(
                    (place, s.position),
                    (Place::Bullet(own), Some(at)) if at != own
                );
                !company && !elsewhere
            })
        })
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
    // What backs it: its own sources (never another position's), and for
    // the headline every position's title.
    let backing: Vec<String> = valid
        .iter()
        .filter(|id| !reasons_cite(&reasons, id))
        .cloned()
        .collect();
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

/// Whether "Yes, keep it" may be offered for a line flagged for `reasons`:
/// never for a company fact used as an achievement.
pub fn keepable(reasons: &[Reason]) -> bool {
    !reasons
        .iter()
        .any(|r| matches!(r, Reason::CompanyFactOutsideSummary { .. }))
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
            checked("Rust Developer and Team Lead", &["sk"], Place::Headline),
            []
        );
        // A month is not a claim; its year is.
        assert_eq!(
            checked("Rebuilt billing in Rust in March", &["w0.h1"], W0),
            []
        );
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
        assert!(!keepable(&checked(text, &cited, W0)));
        assert!(keepable(&checked("Led a team of 12", &["w0.h1"], W0)));
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
