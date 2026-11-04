//! What the app already knows about a person that their profile does not hold
//! yet, as facts to propose (#72). Plain code, no model: every fact comes from
//! words the person wrote, an assumption they confirmed, their own website or
//! a CV they imported, and keeps those words as its quote.
//!
//! - **Tell me about you**: an assumption the person confirmed (*That's
//!   right*, or left unanswered when they said this very portrait is
//!   accurate); a sentence the portrait took from the position or the
//!   person's own words that names something the position does not hold yet
//!   (a number, a tool), when they said the portrait is accurate and it was
//!   made for this very profile; and what they wrote, under "Anything else" or
//!   answering a question, as a fact on a position, a skill or their
//!   background. A change whose quote is not their words, or whose claim
//!   check failed and that they did not settle, is left out and counted,
//!   never proposed.
//! - **Their own website**: what the portrait assumed from it, confirmed the
//!   same way, with the page it was read on.
//! - **A CV read again**: an achievement of a position that the position does
//!   not hold.
//!
//! An item about no position goes on the newest one.

use std::collections::BTreeMap;

use applicamp_kernel::claims::{ClaimKind, Evidence, classified, squash};

use crate::{
    features::{
        about::{
            Choice,
            judge::{self, Change, Field, FlagKind, Resolution, Sentence, SentenceKind},
        },
        grill::role_key,
    },
    matching::latest_position,
    schema::{Area, Profile, Work, match_positions},
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
    /// `yes`, `some` or `no`, for this portrait.
    pub(crate) accuracy: Option<&'a str>,
    pub(crate) changes: &'a [Change],
}

/// What a fact was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// An assumption the person marked *That's right*.
    AssumedRight,
    /// An assumption left unanswered when the person said the portrait is
    /// accurate.
    AssumedAccurate,
    /// A sentence the portrait took from the position or the person's words.
    Known,
    /// A change taken from the person's words.
    Change,
    /// A CV read again.
    Cv,
}

impl Origin {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AssumedRight => "assumed_right",
            Self::AssumedAccurate => "assumed_accurate",
            Self::Known => "known",
            Self::Change => "change",
            Self::Cv => "cv",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "assumed_right" => Some(Self::AssumedRight),
            "assumed_accurate" => Some(Self::AssumedAccurate),
            "known" => Some(Self::Known),
            "change" => Some(Self::Change),
            "cv" => Some(Self::Cv),
            _ => None,
        }
    }
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
    pub(crate) origin: Origin,
    pub(crate) basis: Option<String>,
    pub(crate) area: Area,
}

impl Candidate {
    /// What it is the same as another by: [`key_of`].
    pub(crate) fn key(&self) -> Key {
        key_of(
            self.target,
            self.position.as_ref().map(|(_, k)| k.as_str()),
            &self.text,
        )
    }
}

/// What a proposal is the same as another by: where it goes (a position by
/// its company and start, so a renamed title changes nothing) and its words
/// (no case, no spacing, no closing full stop).
pub(crate) type Key = (FactTarget, String, String);

/// `text` as proposals are compared: squashed, without closing punctuation
/// ("Led 4 engineers." is "Led 4 engineers").
pub(crate) fn norm(text: &str) -> String {
    squash(text)
        .trim_end_matches(['.', '!', ';', ',', ':', '…'])
        .to_owned()
}

/// A position's company and start, from its key (`company|title|start`).
pub(crate) fn place_key(role_key: &str) -> String {
    match role_key.split('|').collect::<Vec<_>>().as_slice() {
        [company, _, start] => format!("{company}|{start}"),
        _ => role_key.to_owned(),
    }
}

/// The key of a proposal for `target`, at the position `role_key` names, saying
/// `text`.
pub(crate) fn key_of(target: FactTarget, role_key: Option<&str>, text: &str) -> Key {
    (
        target,
        role_key.map(place_key).unwrap_or_default(),
        norm(text),
    )
}

/// What [`candidates`] found.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) candidates: Vec<Candidate>,
    /// Changes left out because their quote is not the person's words, or
    /// their claim check failed and was not settled.
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
    let key = norm(text);
    !key.is_empty()
        && (work.facts.iter().any(|f| norm(&f.text) == key)
            || work.highlights.iter().any(|h| norm(h) == key))
}

/// The position of `profile` that `key` (a position's key) names: by the
/// whole key, else by its company and start (its title renamed since).
pub(crate) fn position_by_key(profile: &Profile, key: &str) -> Option<usize> {
    profile
        .work
        .iter()
        .position(|w| role_key(w) == key)
        .or_else(|| {
            let place = place_key(key);
            let mut found = profile
                .work
                .iter()
                .enumerate()
                .filter(|(_, w)| place_key(&role_key(w)) == place);
            match (found.next(), found.next()) {
                (Some((at, _)), None) => Some(at),
                _ => None,
            }
        })
}

/// The position an item made for `key` is about now: by its key, else (an
/// item about no position) the newest one.
fn place_of(profile: &Profile, key: Option<&str>) -> Option<usize> {
    match key {
        Some(key) => position_by_key(profile, key),
        None => latest_position(&profile.work),
    }
}

fn on_position(profile: &Profile, at: usize) -> Option<(usize, String)> {
    profile.work.get(at).map(|work| (at, role_key(work)))
}

/// The one position a sentence's sources name, when it may be read for a
/// fact: every source is a position (`w2`), a fact the person kept on it
/// (`w2f0`) or their own words (`p.summary`, `p.background`), and the
/// positions they name are one. Never the goals, the years, the skills list,
/// the headline, education, or anything read on a website.
fn cited_position(ids: &[String]) -> Option<usize> {
    let mut found: Option<usize> = None;
    for id in ids {
        if id == "p.summary" || id == "p.background" {
            continue;
        }
        let rest = id.strip_prefix('w')?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let tail = &rest[digits.len()..];
        if !(tail.is_empty()
            || tail
                .strip_prefix('f')
                .is_some_and(|f| f.parse::<u32>().is_ok()))
        {
            return None;
        }
        let at = digits.parse::<usize>().ok()?;
        match found {
            Some(seen) if seen != at => return None,
            _ => found = Some(at),
        }
    }
    found
}

/// How the person confirmed the assumption `sentence`, if they did: *That's
/// right*, or no answer at all when they said this portrait is accurate.
fn confirmed(sentence: &Sentence, about: &AboutKnown<'_>) -> Option<Origin> {
    match about.choices.get(&sentence.id) {
        Some(Choice::Right) => Some(Origin::AssumedRight),
        Some(Choice::Wrong | Choice::Later) => None,
        None => (about.accuracy == Some("yes")).then_some(Origin::AssumedAccurate),
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
                let Some(origin) = confirmed(sentence, about) else {
                    continue;
                };
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
                    origin,
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
                    origin: Origin::Known,
                    basis: None,
                    area: sentence.topic,
                });
            }
            // Facts the person wrote or confirmed: on the profile already.
            SentenceKind::Told => {}
        }
    }
}

/// The words a change is proposed with, when it may be: theirs, quoted from
/// what they wrote. A quote the check could not find in their words is never
/// proposed, settled or not; "Use my own words" makes their words the value
/// and the quote. A claim their words do not say must be settled, or said by
/// their profile.
fn quote_of(change: &Change, profile: &serde_json::Value) -> Option<String> {
    if change.resolution == Some(Resolution::OwnWords) {
        return Some(change.value.clone());
    }
    if change.flags.iter().any(|f| f.kind == FlagKind::Quote) {
        return None;
    }
    let settled = !change.open()
        || Evidence::of(&change.quote, profile)
            .unsupported(&change.value)
            .is_empty();
    settled.then(|| change.quote.clone())
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
        let Some(quote) = quote_of(change, &evidence) else {
            out.left_out += 1;
            continue;
        };
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
            quote: cut(&quote, MAX_TEXT),
            source: FactSource::YouSaid,
            origin: Origin::Change,
            basis: None,
            area: change.area,
        });
    }
}

/// A CV read again: the achievements its positions have that the profile's
/// do not, each position matched as Merge matches it.
fn from_cv(profile: &Profile, cv: &Profile, out: &mut Found) {
    for (theirs, mine) in cv.work.iter().zip(match_positions(&profile.work, &cv.work)) {
        let Some(at) = mine else {
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
                origin: Origin::Cv,
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
            .and_then(|(_, key)| position_by_key(profile, key))
            .and_then(|at| profile.work.get(at))
            .is_none_or(|work| holds(work, &candidate.text)),
        FactTarget::Skills => profile
            .skills
            .iter()
            .any(|s| applicamp_kernel::tech::same_skill(&s.name, &candidate.text)),
        FactTarget::Background => profile
            .basics
            .background
            .as_deref()
            .is_some_and(|b| norm(b).contains(&norm(&candidate.text))),
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
    let mut seen: Vec<Key> = Vec::new();
    found.candidates.retain(|candidate| {
        if already_held(profile, candidate) {
            return false;
        }
        let key = candidate.key();
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
        features::about::judge::Flag,
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
                summary: Some("I run billing on Kafka.".into()),
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
                    end_date: Some("2021-02".into()),
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
        assert_eq!(first.origin, Origin::AssumedRight);
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
        assert_eq!(found.candidates[1].origin, Origin::AssumedAccurate);
        let site = &found.candidates[2];
        assert_eq!(
            site.basis.as_deref(),
            Some("https://alex.example.com/projects"),
            "the page it was read on"
        );
        assert_eq!(
            site.position.as_ref().map(|p| p.0),
            Some(0),
            "about no position: on the newest one"
        );
    }

    #[test]
    fn an_item_about_no_position_goes_on_the_newest_one() {
        // The newest position listed second.
        let mut p = profile();
        p.work.reverse();
        let portrait = vec![assumed("s1", "Ran the on-call rota", None, &[])];
        let choices = BTreeMap::from([("s1".to_owned(), Choice::Right)]);
        let about = AboutKnown {
            version: Some(3),
            portrait: &portrait,
            choices: &choices,
            accuracy: None,
            changes: &[],
        };
        let found = candidates(&p, 3, Some(&about), None);
        assert_eq!(found.candidates[0].position.as_ref().map(|p| p.0), Some(1));
    }

    #[test]
    fn a_profile_sentence_adds_only_from_the_position_or_the_persons_words() {
        let p = profile();
        let portrait = vec![
            // Says only what the position says.
            known("s1", "At Example Co you run the billing platform.", &["w0"]),
            // A tool from the person's own About text, about this position.
            known(
                "s2",
                "At Example Co you run billing on Kafka.",
                &["w0", "p.summary"],
            ),
            // A tool from the skills list, the goals or the years: never.
            known(
                "s3",
                "At Sample Labs you used Rust for the pipeline.",
                &["w1", "skills"],
            ),
            known(
                "s4",
                "At Sample Labs the job must have Rust.",
                &["w1", "goals"],
            ),
            known(
                "s5",
                "You worked at Sample Labs for about 4 years.",
                &["w1", "p.years"],
            ),
            // Two positions: whose would it be?
            known("s6", "You used Kafka at both, for 4 years.", &["w0", "w1"]),
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
                "At Example Co you run billing on Kafka.",
                FactSource::YouConfirmed
            )]
        );
        assert_eq!(found.candidates[0].origin, Origin::Known);
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
        // "It's true" on a quote that is not theirs: still not their words.
        let mut not_theirs_settled = not_theirs.clone();
        not_theirs_settled.id = "not-theirs-settled".into();
        not_theirs_settled.value = "Ran the platform team".into();
        not_theirs_settled.resolution = Some(Resolution::True);
        // "Use my own words": their words are the fact and the quote.
        let mut own = change(
            Field::RoleFact,
            "I ran on-call for two years",
            "ran on-call",
            Some(1),
        );
        own.resolution = Some(Resolution::OwnWords);
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
            not_theirs_settled,
            own,
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
                ("I ran on-call for two years", FactSource::YouSaid),
                ("Elixir", FactSource::YouSaid),
                ("Ran a restaurant for 6 years", FactSource::YouSaid),
            ],
            "Postgres is PostgreSQL; a summary is the wizard's"
        );
        assert_eq!(
            found.left_out, 3,
            "an unsettled claim, a quote not theirs, settled or not"
        );
        assert_eq!(found.candidates[2].quote, "I ran on-call for two years");
        let skill = &found.candidates[3];
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
                    highlights: vec!["Built the data pipeline.".into(), "Cut costs by 30%".into()],
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
                "Moved invoicing onto PostgreSQL.",
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
                "led 4  engineers.",
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
    fn keys_ignore_closing_punctuation_and_titles() {
        let key = role_key(&profile().work[1]);
        let renamed = role_key(&work("Sample Labs", "Senior Engineer", "2017-06"));
        assert_eq!(
            key_of(FactTarget::Position, Some(&key), "Cut costs by 30%."),
            key_of(FactTarget::Position, Some(&renamed), "cut costs by 30%")
        );
        // A renamed title still finds its position.
        let mut p = profile();
        p.work[1].position = Some("Senior Engineer".into());
        assert_eq!(position_by_key(&p, &key), Some(1));
    }

    #[test]
    fn the_cited_position_is_one_position_from_its_own_words() {
        let ids = |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(cited_position(&ids(&["w2", "w2f0", "p.summary"])), Some(2));
        assert_eq!(cited_position(&ids(&["w1", "w2"])), None);
        assert_eq!(cited_position(&ids(&["w2", "skills"])), None);
        assert_eq!(cited_position(&ids(&["w2", "goals"])), None);
        assert_eq!(cited_position(&ids(&["w2", "p.years"])), None);
        assert_eq!(cited_position(&ids(&["p.summary"])), None);
    }
}
