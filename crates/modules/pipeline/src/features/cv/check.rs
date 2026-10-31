//! The fact check: no model, the gate for Approve.
//!
//! A line of the CV may say only what the sources it cites say. With the
//! claim rules of the draft source check (`applicamp_kernel::claims`: a
//! number, a technology, an acronym, a name must be in the evidence):
//!
//! - every claim of a line must be in the text of the sources it cites (and
//!   the head of their positions, and the company's name for a company
//!   fact); a source id the frame does not have counts as no source;
//! - a company fact may be cited in the summary only: it says how the
//!   person's real experience fits the company, never what they achieved;
//! - every skill must be one the profile lists (aliases count, as in
//!   `applicamp_kernel::tech`).
//!
//! Each line that fails carries the reasons, and is blocked: Approve stays
//! off until none is.

use std::collections::BTreeMap;

use applicamp_kernel::{
    claims::{Evidence, claims},
    tech,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::model::{Content, Frame, Line, SourceKind};

/// Most claims named for one line.
const MAX_CLAIMS: usize = 3;

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
    /// The skill is not on the profile.
    SkillNotOnProfile { skill: String },
}

/// The reasons of every line that fails, by line id (a skill's too).
pub type Checks = BTreeMap<String, Vec<Reason>>;

/// The reasons `line` fails, none when it is traced to the profile.
pub fn check_line(line: &Line, in_summary: bool, frame: &Frame) -> Vec<Reason> {
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
    if !company.is_empty() && !in_summary {
        reasons.push(Reason::CompanyFactOutsideSummary { cites: company });
    }
    let evidence = Evidence::of_texts(frame.evidence(&valid));
    let mut seen: Vec<String> = Vec::new();
    for claim in evidence.unsupported(&line.text) {
        if seen.iter().any(|c| c.eq_ignore_ascii_case(&claim)) {
            continue;
        }
        seen.push(claim.clone());
        if seen.len() > MAX_CLAIMS {
            break;
        }
        reasons.push(Reason::ClaimNotInSources {
            claim,
            cites: valid.clone(),
        });
    }
    reasons
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
pub fn check(content: &Content, frame: &Frame) -> Checks {
    let mut checks = Checks::new();
    for (line, in_summary) in content.lines() {
        let reasons = check_line(line, in_summary, frame);
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
        features::cv::model::{Contact, Role, SkillItem},
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
                        "Rebuilt billing in Rust, cutting invoice runs from 6 hours to 40 minutes"
                            .into(),
                    ],
                    facts: vec![CvFact {
                        id: "11111111-aaaa".into(),
                        text: "Led a team of 4".into(),
                    }],
                    ..CvPosition::default()
                }],
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

    use crate::features::cv::model;

    fn line(text: &str, sources: &[&str]) -> Line {
        Line {
            id: "l1".into(),
            text: text.into(),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            edited: false,
        }
    }

    #[test]
    fn a_claim_in_a_cited_source_passes() {
        let f = frame();
        for text in [
            "Rebuilt billing in Rust, cutting invoice runs from 6 hours to 40 minutes",
            "Cut invoice runs to 40 minutes with Rust",
            "Led a team of 4 at Example Org",
        ] {
            let sources: &[&str] = if text.starts_with("Led") {
                &["w0.f11111111"]
            } else {
                &["w0.h1"]
            };
            assert_eq!(check_line(&line(text, sources), false, &f), [], "{text}");
        }
    }

    #[test]
    fn a_claim_that_no_cited_source_says_fails_and_names_the_claim() {
        let f = frame();
        let reasons = check_line(
            &line("Cut invoice runs from 6 hours to 20 minutes", &["w0.h1"]),
            false,
            &f,
        );
        assert_eq!(
            reasons,
            [Reason::ClaimNotInSources {
                claim: "20".into(),
                cites: vec!["w0.h1".into()],
            }]
        );
        // A claim another source says does not help a line that does not cite it.
        let reasons = check_line(&line("Led a team of 4", &["w0.h1"]), false, &f);
        assert!(matches!(
            reasons.as_slice(),
            [Reason::ClaimNotInSources { claim, .. }] if claim == "4"
        ));
    }

    #[test]
    fn an_unknown_source_counts_as_no_source() {
        let f = frame();
        assert_eq!(
            check_line(&line("Rebuilt billing in Rust", &["w9.h1"]), false, &f),
            [Reason::NoSource]
        );
        assert_eq!(
            check_line(&line("Rebuilt billing in Rust", &[]), false, &f),
            [Reason::NoSource]
        );
        // A known one next to it still has to say it.
        assert_eq!(
            check_line(
                &line("Rebuilt billing in Rust", &["w9.h1", "w0.h1"]),
                false,
                &f
            ),
            []
        );
    }

    #[test]
    fn a_company_fact_is_for_the_summary_only() {
        let f = frame();
        let text = "Builds ledgers for small shops";
        let cited = ["c99999999"];
        assert_eq!(check_line(&line(text, &cited), true, &f), []);
        assert_eq!(
            check_line(&line(text, &cited), false, &f),
            [Reason::CompanyFactOutsideSummary {
                cites: vec!["c99999999".into()]
            }]
        );
        // The company's name is the company's: a summary may say it.
        assert_eq!(
            check_line(
                &line(
                    "Rust engineer for what Example Co builds",
                    &["w0.h1", "c99999999"]
                ),
                true,
                &f
            ),
            []
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
        let checks = check(&content, &f);
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
