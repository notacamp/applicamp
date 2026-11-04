//! The app clears the check's flags before it asks the person (#73, ADR
//! 0010).
//!
//! After a draft is checked, every flagged line (never a skill: whether the
//! person has one is theirs to say) goes to the model in **one** `complete`
//! call ([`super::fix_prompt`]). For each line it answers a rewrite that says
//! only what the line's sources say, or a question with the fact the person's
//! profile would keep if they say yes:
//!
//! - a rewrite is checked like any line; one that passes replaces the line
//!   (marked as fixed by the app, its first words kept), one that does not
//!   is dropped and the line waits on the person;
//! - a question is kept on the line with its fact when that fact backs the
//!   line (it says every claim the line makes) and claims nothing more than
//!   the line or its own sources do; otherwise "Yes, keep it" keeps the
//!   line's own words. A company fact used as an achievement, a summary whose
//!   flagged words only the company says, and another job's line are never
//!   offered for keeping.
//!
//! One round only, nothing retried. The call goes through the same budget and
//! spending limits as the draft's; when it cannot be made, or answers nothing
//! usable, the draft stays as checked and every flagged line waits on the
//! person. No fact reaches the profile here: only the person's "Yes, keep
//! it" does that.

use applicamp_ai::{Gateway, Role};
use applicamp_kernel::claims::{Evidence, classified};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    check::{Checks, Known, cannot_keep, check_line, usable_sources},
    fix_prompt::{self, Flagged, RawFixes},
    judge::{MAX_BULLET, MAX_HEADLINE, MAX_SUMMARY},
    model::{Ask, Content, Frame, Line, Place, Source, SourceKind},
};
use crate::features::brief::{self, Attempt};

/// The most tokens the answer may take.
pub(super) const MAX_TOKENS: u32 = 3_000;
/// Longest question and fact kept, in characters.
const MAX_QUESTION: usize = 200;
pub(super) const MAX_PROPOSAL: usize = 300;
/// The id of the fact a proposal is checked as, before it is kept.
const PROPOSAL: &str = "_proposal";

/// What the round did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fixed {
    /// Lines rewritten to say only what their sources say.
    pub rewritten: u32,
    /// Lines that wait on the person with a question.
    pub asked: u32,
}

/// The longest a line at `place` may be.
const fn most(place: Place) -> usize {
    match place {
        Place::Headline => MAX_HEADLINE,
        Place::Summary => MAX_SUMMARY,
        Place::Bullet(_) => MAX_BULLET,
    }
}

/// `text` trimmed, when it is something and no longer than `most`.
fn kept(text: Option<&str>, most: usize) -> Option<String> {
    let text = text?.trim();
    (!text.is_empty() && text.chars().count() <= most).then(|| text.to_owned())
}

/// Whether `fact`, kept on the line's position as a source beside its usable
/// sources, would let `line` at `place` pass the check, and claims nothing
/// that neither the line nor its own (never the company's) sources say: what
/// the person keeps is what they were asked about.
pub(super) fn backs(
    fact: &str,
    line: &Line,
    place: Place,
    frame: &Frame,
    known: &Known,
    latest: Option<usize>,
) -> bool {
    let said: Vec<String> = classified(&line.text)
        .into_iter()
        .map(|c| c.text.to_lowercase())
        .collect();
    let own: Vec<String> = usable_sources(line, place, frame)
        .into_iter()
        .filter(|id| frame.get(id).is_some_and(|s| s.kind != SourceKind::Company))
        .collect();
    let sources = Evidence::of_texts(frame.evidence(&own));
    if classified(fact)
        .iter()
        .any(|c| !said.contains(&c.text.to_lowercase()) && !sources.says(&c.text))
    {
        return false;
    }
    let mut with = frame.clone();
    let position = match place {
        Place::Bullet(at) => Some(at),
        Place::Headline | Place::Summary => latest,
    };
    with.sources.push(Source {
        id: PROPOSAL.to_owned(),
        kind: SourceKind::Fact,
        text: fact.to_owned(),
        position,
        order: None,
        url: None,
    });
    let mut sources = usable_sources(line, place, frame);
    sources.push(PROPOSAL.to_owned());
    let candidate = Line {
        sources,
        ..line.clone()
    };
    check_line(&candidate, place, &with, known).is_empty()
}

/// Apply the model's `answers` to the flagged lines of `content`. Pure: no
/// call, no storage.
pub(super) fn apply(
    content: &mut Content,
    frame: &Frame,
    known: &Known,
    checks: &Checks,
    answers: RawFixes,
) -> Fixed {
    let mut fixed = Fixed::default();
    let latest = frame.latest;
    for answer in answers.lines {
        let Some(reasons) = checks.get(&answer.id) else {
            continue;
        };
        let Some(place) = content.place_of(&answer.id) else {
            continue;
        };
        let Some(line) = content.lines_mut().find(|l| l.id == answer.id) else {
            continue;
        };
        if line.ask.is_some() || line.fixed_by_app {
            // One round only.
            continue;
        }
        let usable = usable_sources(line, place, frame);
        if let Some(rewrite) = kept(answer.rewrite.as_deref(), most(place))
            && rewrite != line.text
        {
            let candidate = Line {
                text: rewrite.clone(),
                sources: usable.clone(),
                ..line.clone()
            };
            if !usable.is_empty() && check_line(&candidate, place, frame, known).is_empty() {
                line.original = Some(line.text.clone());
                line.text = rewrite;
                line.sources = usable;
                line.fixed_by_app = true;
                fixed.rewritten += 1;
                continue;
            }
        }
        let question = kept(answer.question.as_deref(), MAX_QUESTION);
        let proposal = if cannot_keep(reasons, place, frame).is_none() {
            kept(answer.fact.as_deref(), MAX_PROPOSAL)
                .filter(|fact| backs(fact, line, place, frame, known, latest))
        } else {
            None
        };
        line.ask = Some(Ask { question, proposal });
        fixed.asked += 1;
    }
    fixed
}

/// The flagged lines of `content` (never a skill), with what they may use.
fn flagged<'a>(content: &'a Content, frame: &Frame, checks: &'a Checks) -> Vec<Flagged<'a>> {
    content
        .placed_lines()
        .filter_map(|(line, place)| {
            let reasons = checks.get(&line.id)?;
            Some(Flagged {
                line,
                place,
                reasons,
                sources: usable_sources(line, place, frame),
            })
        })
        .collect()
}

/// Clear what it can of `content`'s flags with **one model call** (none when
/// no line is flagged), and leave a question on the rest. Never fails the
/// draft: a call that cannot be made or answers nothing usable leaves it as
/// it was. Also says what the call cost, in US dollars.
#[allow(clippy::too_many_arguments)]
pub(super) async fn fix(
    gateway: &Gateway,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
    content: &mut Content,
    frame: &Frame,
    known: &Known,
    checks: &Checks,
    language: &str,
) -> (Fixed, f64) {
    let lines = flagged(content, frame, checks);
    if lines.is_empty() {
        return (Fixed::default(), 0.0);
    }
    let count = lines.len();
    let user = fix_prompt::message(&lines, frame, language);
    let (answers, cost) = match brief::ask_attempt_as::<RawFixes>(
        gateway,
        Role::Complete,
        db,
        account,
        now,
        "pipeline_cv_fix",
        &fix_prompt::schema(),
        fix_prompt::INSTRUCTIONS,
        &user,
        MAX_TOKENS,
    )
    .await
    {
        Ok((Attempt::Done(answers), cost)) => (answers, cost),
        Ok((Attempt::Retry(error), cost)) => {
            // Codes only: the lines are the person's.
            tracing::warn!(
                error = error.code(),
                flagged = count,
                "the app could not fix a CV's flags"
            );
            return (Fixed::default(), cost);
        }
        Err(error) => {
            tracing::warn!(
                error = error.code(),
                flagged = count,
                "the app could not fix a CV's flags"
            );
            return (Fixed::default(), 0.0);
        }
    };
    let fixed = apply(content, frame, known, checks, answers);
    tracing::info!(
        prompt = fix_prompt::VERSION,
        flagged = count,
        rewritten = fixed.rewritten,
        asked = fixed.asked,
        "a CV's flags fixed by the app"
    );
    (fixed, cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        features::cv::{
            check::{self, Reason},
            fix_prompt::RawFix,
            model::{CompanyFact, Contact, Role},
        },
        ports::{CvPosition, CvProfile},
    };

    fn frame() -> Frame {
        Frame::build(
            &CvProfile {
                positions: vec![
                    CvPosition {
                        index: 0,
                        title: Some("Senior Engineer".into()),
                        company: Some("Example Org".into()),
                        highlights: vec![
                            "Cut invoice runs from 6 hours to 40 minutes".into(),
                            "Led a team of 4 engineers".into(),
                        ],
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
                skills: vec!["Rust".into()],
                latest: Some(0),
                ..CvProfile::default()
            },
            "Example Co",
            &[CompanyFact {
                id: "99999999".into(),
                text: "Builds ledgers for small shops".into(),
                url: "https://example.test/".into(),
            }],
        )
    }

    fn line(id: &str, text: &str, sources: &[&str]) -> Line {
        Line {
            id: id.into(),
            text: text.into(),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            ..Line::default()
        }
    }

    fn content() -> Content {
        Content {
            experience: vec![Role {
                position: 0,
                bullets: vec![
                    // A number nobody wrote.
                    line("l1", "Cut invoice runs to 20 minutes", &["w0.h1"]),
                    // A company fact as an achievement.
                    line(
                        "l2",
                        "Led a team of 4 engineers building ledgers for small shops",
                        &["w0.h2", "c99999999"],
                    ),
                    // Another job's line.
                    line("l3", "Built ingestion for 3 partner feeds", &["w1.h1"]),
                ],
            }],
            ..Content::default()
        }
    }

    fn fix(id: &str, rewrite: Option<&str>, question: Option<&str>, fact: Option<&str>) -> RawFix {
        RawFix {
            id: id.into(),
            rewrite: rewrite.map(str::to_owned),
            question: question.map(str::to_owned),
            fact: fact.map(str::to_owned),
        }
    }

    fn run(answers: Vec<RawFix>) -> (Content, Fixed, Checks) {
        let f = frame();
        let mut c = content();
        let known = Known::new(&f, &Contact::default(), "Staff Engineer");
        let checks = check::check(&c, &f, &known);
        assert_eq!(checks.len(), 3, "{checks:?}");
        let fixed = apply(&mut c, &f, &known, &checks, RawFixes { lines: answers });
        let after = check::check(&c, &f, &known);
        (c, fixed, after)
    }

    fn bullet<'a>(c: &'a Content, id: &str) -> &'a Line {
        c.lines().map(|(l, _)| l).find(|l| l.id == id).unwrap()
    }

    #[test]
    fn a_rewrite_that_passes_replaces_the_line_and_keeps_its_first_words() {
        let (c, fixed, after) = run(vec![
            fix("l2", Some("Led a team of 4 engineers"), None, None),
            fix(
                "l3",
                None,
                Some("Did you build ingestion at Example Org?"),
                None,
            ),
        ]);
        assert_eq!(
            fixed,
            Fixed {
                rewritten: 1,
                asked: 1
            }
        );
        let l2 = bullet(&c, "l2");
        assert_eq!(l2.text, "Led a team of 4 engineers");
        assert!(l2.fixed_by_app);
        assert_eq!(
            l2.original.as_deref(),
            Some("Led a team of 4 engineers building ledgers for small shops")
        );
        // The company fact no longer backs it.
        assert_eq!(l2.sources, ["w0.h2"]);
        assert!(!after.contains_key("l2"));
        // The other position's source cannot back a line here: it waits.
        assert!(after.contains_key("l3"));
        assert!(bullet(&c, "l3").ask.is_some());
    }

    #[test]
    fn a_rewrite_that_still_says_too_much_becomes_a_question() {
        let (c, fixed, after) = run(vec![fix(
            "l1",
            Some("Cut invoice runs to 25 minutes"),
            Some("Did invoice runs drop to 20 minutes?"),
            Some("I cut invoice runs to 20 minutes."),
        )]);
        assert_eq!(
            fixed,
            Fixed {
                rewritten: 0,
                asked: 1
            }
        );
        let l1 = bullet(&c, "l1");
        assert_eq!(
            l1.text, "Cut invoice runs to 20 minutes",
            "the line is unchanged"
        );
        assert!(!l1.fixed_by_app);
        assert_eq!(
            l1.ask,
            Some(Ask {
                question: Some("Did invoice runs drop to 20 minutes?".into()),
                proposal: Some("I cut invoice runs to 20 minutes.".into()),
            })
        );
        assert!(matches!(
            after.get("l1").map(Vec::as_slice),
            Some([Reason::ClaimNotInSources { .. }])
        ));
    }

    #[test]
    fn a_fact_that_does_not_back_the_line_is_not_proposed() {
        let (c, _, _) = run(vec![fix(
            "l1",
            None,
            Some("Did invoice runs drop?"),
            Some("I made invoice runs faster."),
        )]);
        let ask = bullet(&c, "l1").ask.clone().unwrap();
        assert_eq!(ask.question.as_deref(), Some("Did invoice runs drop?"));
        assert_eq!(ask.proposal, None, "the line's own words are kept instead");
    }

    #[test]
    fn a_fact_that_claims_more_than_the_line_is_not_proposed() {
        let f = frame();
        let known = Known::new(&f, &Contact::default(), "");
        let l1 = line("l1", "Cut invoice runs to 20 minutes", &["w0.h1"]);
        // The line's claim, and words its own source says: kept as asked.
        assert!(backs(
            "I cut invoice runs from 6 hours to 20 minutes.",
            &l1,
            Place::Bullet(0),
            &f,
            &known,
            f.latest
        ));
        // A tool, a number or a name nobody asked about: never kept by a yes.
        for fact in [
            "I cut invoice runs to 20 minutes with Kafka.",
            "I cut invoice runs to 20 minutes for 300 clients.",
            "I cut invoice runs to 20 minutes at Sample Labs.",
        ] {
            assert!(
                !backs(fact, &l1, Place::Bullet(0), &f, &known, f.latest),
                "{fact}"
            );
        }
        // The company's facts are not the person's sources.
        let summary = line(
            "s1",
            "Rust engineer, cut runs to 20 minutes",
            &["w0.h1", "c99999999"],
        );
        assert!(!backs(
            "I cut runs to 20 minutes for Ledgers of small shops.",
            &summary,
            Place::Summary,
            &f,
            &known,
            f.latest
        ));
    }

    #[test]
    fn a_company_fact_used_as_an_achievement_is_never_proposed() {
        let (c, _, _) = run(vec![fix(
            "l2",
            None,
            Some("Did you build ledgers?"),
            Some("I led a team of 4 engineers building ledgers for small shops."),
        )]);
        assert_eq!(bullet(&c, "l2").ask.clone().unwrap().proposal, None);
    }

    #[test]
    fn answers_for_lines_that_are_not_flagged_or_unknown_are_ignored() {
        let (c, fixed, after) = run(vec![
            fix("nope", Some("Anything"), None, None),
            fix("l1", Some(""), None, None),
        ]);
        assert_eq!(
            fixed,
            Fixed {
                rewritten: 0,
                asked: 1
            }
        );
        assert_eq!(after.len(), 3);
        assert_eq!(bullet(&c, "l1").ask, Some(Ask::default()));
    }

    #[test]
    fn the_lines_own_words_always_back_it() {
        let f = frame();
        let known = Known::new(&f, &Contact::default(), "");
        let l3 = line("l3", "Built ingestion for 3 partner feeds", &["w1.h1"]);
        assert!(backs(&l3.text, &l3, Place::Bullet(0), &f, &known, f.latest));
        let l1 = line("l1", "Cut invoice runs to 20 minutes", &["w0.h1"]);
        assert!(backs(&l1.text, &l1, Place::Bullet(0), &f, &known, f.latest));
        assert!(!backs(
            "I made them faster",
            &l1,
            Place::Bullet(0),
            &f,
            &known,
            f.latest
        ));
    }

    #[test]
    fn the_model_is_shown_the_flagged_lines_and_their_usable_sources_only() {
        let f = frame();
        let c = content();
        let known = Known::new(&f, &Contact::default(), "");
        let checks = check::check(&c, &f, &known);
        let lines = flagged(&c, &f, &checks);
        let message = fix_prompt::message(&lines, &f, "en");
        assert!(
            message.contains(
                "<line id=\"l1\" where=\"a bullet under Senior Engineer · Example Org\">"
            )
        );
        assert!(message.contains("\"20\" is not in its sources"));
        assert!(
            message.contains(
                "<source id=\"w0.h1\">Cut invoice runs from 6 hours to 40 minutes</source>"
            )
        );
        // The company fact and the other job's highlight are not offered.
        assert!(!message.contains("<source id=\"c99999999\">"));
        assert!(!message.contains("<source id=\"w1.h1\">"));
        assert!(message.contains("it uses what the person did in another job"));
    }

    #[test]
    fn the_companys_facts_are_shown_as_the_companys_and_never_kept() {
        let f = frame();
        let known = Known::new(&f, &Contact::default(), "");
        let mut c = Content {
            summary: Some(line(
                "s1",
                "Engineer for Ledgers of 30 small shops",
                &["w0.h1", "c99999999"],
            )),
            ..Content::default()
        };
        let checks = check::check(&c, &f, &known);
        assert!(checks.contains_key("s1"), "{checks:?}");
        let lines = flagged(&c, &f, &checks);
        let message = fix_prompt::message(&lines, &f, "en");
        assert!(
            message.contains(
                "<company_source id=\"c99999999\">Builds ledgers for small shops</company_source>"
            ),
            "{message}"
        );
        assert!(message.contains("<source id=\"w0.h1\">"));
        // "30" is the person's to say: its question may keep a fact.
        apply(
            &mut c,
            &f,
            &known,
            &checks,
            RawFixes {
                lines: vec![fix(
                    "s1",
                    None,
                    Some("Did you work for 30 small shops?"),
                    Some("I worked for 30 small shops."),
                )],
            },
        );
        assert_eq!(
            c.summary
                .as_ref()
                .unwrap()
                .ask
                .as_ref()
                .unwrap()
                .proposal
                .as_deref(),
            Some("I worked for 30 small shops.")
        );
    }
}
