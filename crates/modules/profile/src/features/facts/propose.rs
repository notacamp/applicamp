//! What the app already knows about a person that their profile does not hold
//! yet, as facts to propose (#72). Plain code, no model: every fact comes from
//! words the person wrote, an assumption they confirmed, their own website or
//! a CV they imported, and keeps those words as its quote.
//!
//! - **Tell me about you**: an assumption the person confirmed (*That's
//!   right*, or left unanswered when they said the portrait is accurate);
//!   a sentence from the profile that names something its position does not
//!   hold yet (a number, a tool), when they said the portrait is accurate and
//!   it was made for this very profile; and what they wrote, under "Anything
//!   else" or answering a question, as a fact on a position, a skill or their
//!   background. A change whose claim check failed and that they did not
//!   settle is left out and counted, never proposed.
//! - **Their own website**: what the portrait assumed from it, confirmed the
//!   same way, with the page it was read on.
//! - **A CV read again**: an achievement of a position that the position does
//!   not hold.

use std::collections::BTreeMap;

use applicamp_kernel::claims::{ClaimKind, Evidence, classified, squash};

use crate::{
    features::{
        about::{
            Choice,
            judge::{self, Change, Field, Sentence, SentenceKind},
        },
        grill::role_key,
    },
    schema::{Area, Profile, Work, same_position},
};

use super::{FactSource, FactTarget};

/// Longest fact, in characters.
pub(crate) const MAX_TEXT: usize = 600;
/// Longest basis, in characters.
const MAX_BASIS: usize = 300;

/// What Tell me about you holds while it is pending: the portrait, the
/// person's choices and answer to "Is this accurate?", and the changes taken
/// from what they wrote.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AboutKnown<'a> {
    /// The profile version the portrait was made for.
    pub(crate) version: Option<i32>,
    pub(crate) portrait: &'a [Sentence],
    pub(crate) choices: &'a BTreeMap<String, Choice>,
    /// `yes`, `some` or `no`.
    pub(crate) accuracy: Option<&'a str>,
    pub(crate) changes: &'a [Change],
}

/// A fact to propose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) target: FactTarget,
    /// For a position: its place in `work` and its key.
    pub(crate) position: Option<(usize, String)>,
    pub(crate) text: String,
    pub(crate) quote: String,
    pub(crate) source: FactSource,
    pub(crate) basis: Option<String>,
    pub(crate) area: Area,
}

/// What [`candidates`] found.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) candidates: Vec<Candidate>,
    /// Changes left out because their claim check failed and was not settled.
    pub(crate) left_out: u32,
}

fn cut(text: &str, max: usize) -> String {
    judge::cut(text, max)
}

/// Everything `work` says, as evidence of what the position already holds.
fn position_text(work: &Work) -> Vec<String> {
    let mut texts: Vec<String> = [
        &work.name,
        &work.position,
        &work.location,
        &work.start_date,
        &work.end_date,
        &work.summary,
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect();
    texts.extend(work.highlights.iter().cloned());
    texts.extend(work.facts.iter().map(|f| f.text.clone()));
    texts
}

/// Whether `work` already says `text` (as a fact or an achievement).
pub(crate) fn holds(work: &Work, text: &str) -> bool {
    let key = squash(text);
    !key.is_empty()
        && (work.facts.iter().any(|f| squash(&f.text) == key)
            || work.highlights.iter().any(|h| squash(h) == key))
}

/// The position an item made for `key` is about now: by its key, else (an
/// item about no position) the first one, as Save places it.
fn place_of(profile: &Profile, key: Option<&str>) -> Option<usize> {
    if profile.work.is_empty() {
        return None;
    }
    match key {
        Some(key) => profile.work.iter().position(|w| role_key(w) == key),
        None => Some(0),
    }
}

fn on_position(profile: &Profile, at: usize) -> Option<(usize, String)> {
    profile.work.get(at).map(|work| (at, role_key(work)))
}

/// The one position the sources `ids` cite (`w2`, `w2f0`); none when they cite
/// none or more than one.
fn cited_position(ids: &[String]) -> Option<usize> {
    let mut found: Option<usize> = None;
    for id in ids {
        let Some(rest) = id.strip_prefix('w') else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(at) = digits.parse::<usize>() else {
            continue;
        };
        match found {
            Some(seen) if seen != at => return None,
            _ => found = Some(at),
        }
    }
    found
}

/// Whether the person confirmed the assumption `sentence`: *That's right*, or
/// no answer at all when they said the portrait is accurate.
fn confirmed(sentence: &Sentence, about: &AboutKnown<'_>) -> bool {
    match about.choices.get(&sentence.id) {
        Some(Choice::Right) => true,
        Some(Choice::Wrong | Choice::Later) => false,
        None => about.accuracy == Some("yes"),
    }
}

/// Whether a sentence was read on the person's own website.
fn from_website(sentence: &Sentence) -> bool {
    sentence.sources.iter().any(|id| id.starts_with("site"))
}

/// The portrait's sentences as facts.
fn from_portrait(profile: &Profile, version: i32, about: &AboutKnown<'_>, out: &mut Found) {
    let current = about.version == Some(version);
    for sentence in about.portrait {
        match sentence.kind {
            SentenceKind::Assumed => {
                if !confirmed(sentence, about) {
                    continue;
                }
                let Some(at) = place_of(profile, sentence.role_key.as_deref()) else {
                    continue;
                };
                let claim = cut(&judge::claim_of(sentence), MAX_TEXT);
                if claim.is_empty() {
                    continue;
                }
                let (source, basis) = if from_website(sentence) {
                    (FactSource::YourWebsite, sentence.urls.first().cloned())
                } else {
                    (
                        FactSource::YouConfirmed,
                        sentence
                            .why
                            .clone()
                            .or_else(|| sentence.urls.first().cloned()),
                    )
                };
                out.candidates.push(Candidate {
                    target: FactTarget::Position,
                    position: on_position(profile, at),
                    quote: claim.clone(),
                    text: claim,
                    source,
                    basis: basis.map(|b| cut(&b, MAX_BASIS)),
                    area: sentence.topic,
                });
            }
            SentenceKind::Known => {
                // Its sources name positions by their place: only for the
                // profile it was made from, and only when the person said it
                // is accurate.
                if !current || about.accuracy != Some("yes") {
                    continue;
                }
                let Some(at) = cited_position(&sentence.sources) else {
                    continue;
                };
                let Some(work) = profile.work.get(at) else {
                    continue;
                };
                let held = Evidence::of_texts(position_text(work));
                let adds = classified(&sentence.text).into_iter().any(|claim| {
                    matches!(claim.kind, ClaimKind::Number | ClaimKind::Tool)
                        && !held.says(&claim.text)
                });
                if !adds {
                    continue;
                }
                let text = cut(&sentence.text, MAX_TEXT);
                out.candidates.push(Candidate {
                    target: FactTarget::Position,
                    position: on_position(profile, at),
                    quote: text.clone(),
                    text,
                    source: FactSource::YouConfirmed,
                    basis: None,
                    area: sentence.topic,
                });
            }
            // Facts the person wrote or confirmed: on the profile already.
            SentenceKind::Told => {}
        }
    }
}

/// Whether a change may be proposed: no flag, or one the person settled, or
/// only claims their profile says (never a quote that is not theirs).
fn passes(change: &Change, profile: &serde_json::Value) -> bool {
    if !change.open() {
        return true;
    }
    change
        .flags
        .iter()
        .all(|f| f.kind == judge::FlagKind::Claim)
        && Evidence::of(&change.quote, profile)
            .unsupported(&change.value)
            .is_empty()
}

/// What the person wrote, as facts on positions, skills and a background.
fn from_changes(profile: &Profile, changes: &[Change], out: &mut Found) {
    let evidence = serde_json::to_value(profile).unwrap_or_default();
    for change in changes {
        let (target, position) = match change.field {
            Field::RoleFact => match place_of(profile, change.role_key.as_deref()) {
                Some(at) => (FactTarget::Position, on_position(profile, at)),
                None => continue,
            },
            Field::Skill => (FactTarget::Skills, None),
            Field::Background => (FactTarget::Background, None),
            // The rest is the wizard's to save: summary, headline, goals,
            // languages, education.
            _ => continue,
        };
        if !passes(change, &evidence) {
            out.left_out += 1;
            continue;
        }
        let max = if target == FactTarget::Skills {
            judge::MAX_SKILL
        } else {
            MAX_TEXT
        };
        let text = cut(&change.value, max);
        if text.is_empty() {
            continue;
        }
        out.candidates.push(Candidate {
            target,
            position,
            text,
            quote: cut(&change.quote, MAX_TEXT),
            source: FactSource::YouSaid,
            basis: None,
            area: change.area,
        });
    }
}

/// A CV read again: the achievements its positions have that the profile's
/// do not.
fn from_cv(profile: &Profile, cv: &Profile, out: &mut Found) {
    for theirs in &cv.work {
        let Some(at) = profile
            .work
            .iter()
            .position(|mine| same_position(mine, theirs))
        else {
            continue;
        };
        for highlight in &theirs.highlights {
            let text = cut(highlight, MAX_TEXT);
            if text.is_empty() {
                continue;
            }
            out.candidates.push(Candidate {
                target: FactTarget::Position,
                position: on_position(profile, at),
                quote: text.clone(),
                text,
                source: FactSource::YourCv,
                basis: None,
                area: Area::Results,
            });
        }
    }
}

/// Whether the profile holds `candidate` already: a fact or achievement of
/// its position, one of its skills, or its background.
pub(crate) fn already_held(profile: &Profile, candidate: &Candidate) -> bool {
    match candidate.target {
        FactTarget::Position => candidate
            .position
            .as_ref()
            .and_then(|(_, key)| profile.work.iter().find(|w| role_key(w) == *key))
            .is_none_or(|work| holds(work, &candidate.text)),
        FactTarget::Skills => profile
            .skills
            .iter()
            .any(|s| applicamp_kernel::tech::same_skill(&s.name, &candidate.text)),
        FactTarget::Background => profile
            .basics
            .background
            .as_deref()
            .is_some_and(|b| squash(b).contains(&squash(&candidate.text))),
    }
}

/// Every fact the app can propose for `profile` (at `version`) from what Tell
/// me about you holds and a CV read again, none the profile holds already and
/// none twice.
pub(crate) fn candidates(
    profile: &Profile,
    version: i32,
    about: Option<&AboutKnown<'_>>,
    cv: Option<&Profile>,
) -> Found {
    let mut found = Found::default();
    if let Some(about) = about {
        from_portrait(profile, version, about, &mut found);
        from_changes(profile, about.changes, &mut found);
    }
    if let Some(cv) = cv {
        from_cv(profile, cv, &mut found);
    }
    let mut seen: Vec<(FactTarget, Option<String>, String)> = Vec::new();
    found.candidates.retain(|candidate| {
        if already_held(profile, candidate) {
            return false;
        }
        let key = (
            candidate.target,
            candidate.position.as_ref().map(|(_, key)| key.clone()),
            squash(&candidate.text),
        );
        if seen.contains(&key) {
            return false;
        }
        seen.push(key);
        true
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        features::about::judge::{Flag, FlagKind, Resolution},
        schema::{Basics, FactStatus, ProfileFact, Skill},
    };

    fn work(company: &str, title: &str, start: &str) -> Work {
        Work {
            name: Some(company.into()),
            position: Some(title.into()),
            start_date: Some(start.into()),
            summary: Some("Runs the billing platform.".into()),
            ..Work::default()
        }
    }

    fn profile() -> Profile {
        Profile {
            basics: Basics {
                background: Some("Six years in events.".into()),
                ..Basics::default()
            },
            work: vec![
                Work {
                    facts: vec![ProfileFact {
                        id: "f1".into(),
                        text: "Moved invoicing onto PostgreSQL".into(),
                        quote: "Moved invoicing onto PostgreSQL".into(),
                        area: Area::Results,
                        kept_at: chrono::Utc::now(),
                        status: FactStatus::Told,
                        basis: None,
                    }],
                    ..work("Example Co", "Lead Engineer", "2021-03")
                },
                Work {
                    highlights: vec!["Built the data pipeline".into()],
                    ..work("Sample Labs", "Engineer", "2017-06")
                },
            ],
            skills: vec![Skill {
                name: "PostgreSQL".into(),
                level: None,
            }],
            ..Profile::default()
        }
    }

    fn assumed(id: &str, text: &str, position: Option<usize>, sources: &[&str]) -> Sentence {
        let p = profile();
        Sentence {
            id: id.into(),
            text: format!("I assume {text}"),
            kind: SentenceKind::Assumed,
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            why: Some("Lead titles usually mean a team.".into()),
            claim: Some(text.into()),
            downgraded: false,
            topic: Area::People,
            position: position.map(|p| u32::try_from(p).unwrap()),
            role_key: position.map(|at| role_key(&p.work[at])),
            urls: if sources.iter().any(|s| s.starts_with("site")) {
                vec!["https://alex.example.com/projects".into()]
            } else {
                Vec::new()
            },
            new: false,
        }
    }

    fn known(id: &str, text: &str, sources: &[&str]) -> Sentence {
        Sentence {
            id: id.into(),
            text: text.into(),
            kind: SentenceKind::Known,
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            why: None,
            claim: None,
            downgraded: false,
            topic: Area::Context,
            position: None,
            role_key: None,
            urls: Vec::new(),
            new: false,
        }
    }

    fn change(field: Field, value: &str, quote: &str, position: Option<usize>) -> Change {
        Change {
            id: format!("c-{value}"),
            field,
            value: value.into(),
            position: position.map(|p| u32::try_from(p).unwrap()),
            role_key: position.map(|at| role_key(&profile().work[at])),
            area: Area::Results,
            quote: quote.into(),
            sentence: value.into(),
            flags: Vec::new(),
            resolution: None,
            origin: None,
        }
    }

    fn texts(found: &Found) -> Vec<(&str, FactSource)> {
        found
            .candidates
            .iter()
            .map(|c| (c.text.as_str(), c.source))
            .collect()
    }

    #[test]
    fn assumptions_the_person_confirmed_are_proposed() {
        let p = profile();
        let portrait = vec![
            assumed("s1", "Led a team of 3 to 6 engineers", Some(0), &[]),
            assumed("s2", "Worked on payments", Some(0), &[]),
            assumed("s3", "Worked on bookings", Some(1), &[]),
            assumed("s4", "Ran the on-call rota", Some(1), &[]),
            assumed(
                "s5",
                "Built an open-source billing toolkit",
                None,
                &["site0"],
            ),
        ];
        let choices = BTreeMap::from([
            ("s1".to_owned(), Choice::Right),
            ("s2".to_owned(), Choice::Wrong),
            ("s3".to_owned(), Choice::Later),
        ]);
        // Without "Is this accurate? Yes", only the one marked right.
        let about = AboutKnown {
            version: Some(3),
            portrait: &portrait,
            choices: &choices,
            accuracy: None,
            changes: &[],
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(
            texts(&found),
            [("Led a team of 3 to 6 engineers", FactSource::YouConfirmed)]
        );
        let first = &found.candidates[0];
        assert_eq!(first.position, Some((0, role_key(&p.work[0]))));
        assert_eq!(
            first.basis.as_deref(),
            Some("Lead titles usually mean a team.")
        );
        assert_eq!(first.source.status(), FactStatus::ConfirmedAssumption);

        // "Yes, mostly": the ones left unanswered too, never wrong or later.
        let about = AboutKnown {
            accuracy: Some("yes"),
            ..about
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(
            texts(&found),
            [
                ("Led a team of 3 to 6 engineers", FactSource::YouConfirmed),
                ("Ran the on-call rota", FactSource::YouConfirmed),
                (
                    "Built an open-source billing toolkit",
                    FactSource::YourWebsite
                ),
            ]
        );
        let site = &found.candidates[2];
        assert_eq!(
            site.basis.as_deref(),
            Some("https://alex.example.com/projects"),
            "the page it was read on"
        );
        assert_eq!(site.position.as_ref().map(|p| p.0), Some(0), "as Save does");
    }

    #[test]
    fn a_profile_sentence_that_adds_a_number_or_a_tool_is_proposed() {
        let p = profile();
        let portrait = vec![
            // Says only what the position says.
            known("s1", "At Example Co you run the billing platform.", &["w0"]),
            // Names a tool the position does not.
            known(
                "s2",
                "At Sample Labs you used Kafka for the pipeline.",
                &["w1", "skills"],
            ),
            // A number, but about no one position.
            known("s3", "You have worked for about 9 years.", &["p.years"]),
            // Two positions: whose would it be?
            known("s4", "You used Rust at both, for 4 years.", &["w0", "w1"]),
        ];
        let choices = BTreeMap::new();
        let about = AboutKnown {
            version: Some(3),
            portrait: &portrait,
            choices: &choices,
            accuracy: Some("yes"),
            changes: &[],
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(
            texts(&found),
            [(
                "At Sample Labs you used Kafka for the pipeline.",
                FactSource::YouConfirmed
            )]
        );
        assert_eq!(found.candidates[0].position.as_ref().map(|p| p.0), Some(1));
        // A portrait of another profile version names positions by places that
        // may have moved: nothing from it.
        assert!(candidates(&p, 4, Some(&about), None).candidates.is_empty());
        // Without "Yes, mostly", nothing either.
        let unsure = AboutKnown {
            accuracy: Some("some"),
            ..about
        };
        assert!(candidates(&p, 3, Some(&unsure), None).candidates.is_empty());
    }

    #[test]
    fn what_the_person_wrote_becomes_facts_skills_and_a_background() {
        let p = profile();
        let mut flagged = change(
            Field::RoleFact,
            "Cut refunds by 80%",
            "cut refunds a lot",
            Some(0),
        );
        flagged.flags = vec![Flag {
            kind: FlagKind::Claim,
            claim: "80%".into(),
        }];
        let mut settled = flagged.clone();
        settled.id = "settled".into();
        settled.value = "Cut refunds by half".into();
        settled.resolution = Some(Resolution::True);
        let mut not_theirs = change(Field::RoleFact, "Ran the team", "I ran it", Some(1));
        not_theirs.flags = vec![Flag {
            kind: FlagKind::Quote,
            claim: String::new(),
        }];
        let changes = vec![
            change(
                Field::RoleFact,
                "Led 4 engineers",
                "I led 4 engineers",
                Some(0),
            ),
            flagged,
            settled,
            not_theirs,
            change(
                Field::Skill,
                "Elixir",
                "I use Rails, Elixir and Postgres",
                None,
            ),
            change(
                Field::Skill,
                "Postgres",
                "I use Rails, Elixir and Postgres",
                None,
            ),
            change(
                Field::Background,
                "Ran a restaurant for 6 years",
                "I ran a restaurant for 6 years",
                None,
            ),
            change(
                Field::Summary,
                "A backend engineer",
                "backend engineer",
                None,
            ),
        ];
        let choices = BTreeMap::new();
        let about = AboutKnown {
            version: Some(1),
            portrait: &[],
            choices: &choices,
            accuracy: None,
            changes: &changes,
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(
            texts(&found),
            [
                ("Led 4 engineers", FactSource::YouSaid),
                ("Cut refunds by half", FactSource::YouSaid),
                ("Elixir", FactSource::YouSaid),
                ("Ran a restaurant for 6 years", FactSource::YouSaid),
            ],
            "Postgres is PostgreSQL; a summary is the wizard's"
        );
        assert_eq!(found.left_out, 2, "an unsettled claim, a quote not theirs");
        let skill = &found.candidates[2];
        assert_eq!(skill.target, FactTarget::Skills);
        assert_eq!(skill.position, None);
        assert_eq!(skill.quote, "I use Rails, Elixir and Postgres");
        assert_eq!(found.candidates[0].source.status(), FactStatus::Told);
    }

    #[test]
    fn a_cv_read_again_proposes_what_its_positions_say_that_the_profile_does_not() {
        let p = profile();
        let cv = Profile {
            work: vec![
                Work {
                    highlights: vec!["Built the data pipeline".into(), "Cut costs by 30%".into()],
                    ..work("Sample Labs", "Engineer", "2017-06")
                },
                Work {
                    highlights: vec!["Shipped it".into()],
                    ..work("Nowhere Inc", "Intern", "2015-01")
                },
            ],
            ..Profile::default()
        };
        let found = candidates(&p, 3, None, Some(&cv));
        assert_eq!(texts(&found), [("Cut costs by 30%", FactSource::YourCv)]);
        assert_eq!(found.candidates[0].position.as_ref().map(|p| p.0), Some(1));
    }

    #[test]
    fn what_the_profile_holds_is_never_proposed_and_nothing_twice() {
        let p = profile();
        let changes = vec![
            change(
                Field::RoleFact,
                "Moved invoicing onto PostgreSQL",
                "Moved invoicing onto PostgreSQL",
                Some(0),
            ),
            change(
                Field::RoleFact,
                "Led 4 engineers",
                "I led 4 engineers",
                Some(0),
            ),
            change(
                Field::RoleFact,
                "led 4  engineers",
                "I led 4 engineers",
                Some(0),
            ),
            change(
                Field::Background,
                "Six years in events.",
                "six years in events",
                None,
            ),
        ];
        let choices = BTreeMap::new();
        let about = AboutKnown {
            version: Some(3),
            portrait: &[],
            choices: &choices,
            accuracy: None,
            changes: &changes,
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(texts(&found), [("Led 4 engineers", FactSource::YouSaid)]);
    }

    #[test]
    fn the_cited_position_is_the_one_position_cited() {
        let ids = |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(cited_position(&ids(&["w2", "w2f0", "skills"])), Some(2));
        assert_eq!(cited_position(&ids(&["w1", "w2"])), None);
        assert_eq!(cited_position(&ids(&["skills", "p.years"])), None);
    }
}
