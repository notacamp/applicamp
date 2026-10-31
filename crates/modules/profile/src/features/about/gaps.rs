//! What is missing (#44): the gaps in what Applicamp knows about a person,
//! decided in plain code from the profile, the goals, the portrait's
//! assumptions and what the person has told so far. **No model decides what
//! is asked**; the model only words the question (see [`super::questions`]).
//!
//! A gap is `{id, kind, position?, area?, basis}`, where `basis` is what was
//! assumed or what is missing, in plain English, as data for the model to
//! put in the person's language. Ranked, the gaps are:
//!
//! 1. assumptions the person chose "Ask me later" for (with "Not really" on
//!    "Is this accurate?", the ones they marked wrong too);
//! 2. the most recent role's results without a number;
//! 3. empty skills;
//! 4. a lead or manager title without a team size, results without a number
//!    on the other recent roles, technologies the roles name that are not
//!    skills, a claim from "Anything else" no role backs, an empty summary,
//!    headline or background, goals not answered, and each area of each
//!    recent role nothing has been kept about.
//!
//! Context (what the company sells, to whom) is never a gap: it comes from
//! the employer's own website or from an assumption to confirm, and what can
//! be looked up is not asked.

use std::collections::{BTreeMap, BTreeSet};

use applicamp_kernel::tech;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    Accuracy, Choice,
    judge::{Change, Field, Sentence, SentenceKind},
};
use crate::{
    features::grill,
    goals::Goals,
    schema::{Area, FactStatus, Profile, ProfileFact, Skill, Work},
    strength,
};

/// Most questions when "Is this accurate?" says "Some of it's wrong" or has
/// no answer.
pub const MAX_QUESTIONS: usize = 12;
/// Most questions after "Yes, mostly".
pub const MAX_QUESTIONS_ACCURATE: usize = 8;
/// Most questions after "Not really": more about the portrait.
pub const MAX_QUESTIONS_INACCURATE: usize = 16;

/// How many of the latest positions are asked about.
const RECENT: usize = strength::RECENT_ROLES;
/// The areas asked about, besides context (never asked) and results (its own
/// gap, because it needs a number).
const AREAS: [Area; 4] = [
    Area::YourPart,
    Area::StackScale,
    Area::HardProblems,
    Area::Stories,
];
/// Most technologies one "your roles mention…" gap names.
const MAX_FOUND: usize = 5;

/// What a gap is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    /// An assumption the person left for later (or marked wrong).
    Assumed,
    /// A recent position states no result with a number.
    Results,
    /// A lead or manager title, no team size.
    TeamSize,
    /// An area of a recent position nothing is kept about.
    Area,
    /// The profile lists no skills.
    Skills,
    /// Technologies the roles name that are not skills.
    SkillsFound,
    /// A claim from "Anything else" that no role backs.
    Claim,
    /// No summary.
    Summary,
    /// No headline.
    Headline,
    /// No background.
    Background,
    /// The roles they want next are not answered.
    Goals,
}

/// One thing Applicamp does not know, or assumes and is not sure of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Stable for a profile version (`results:0`, `assumed:s3`, `skills`…).
    pub id: String,
    pub kind: GapKind,
    /// The position it is about (its place in `work`).
    pub position: Option<u32>,
    pub area: Option<Area>,
    /// What was assumed, or what is missing, in plain English.
    pub basis: String,
    /// For skills: the names concerned.
    #[serde(default)]
    pub names: Vec<String>,
}

/// What the gaps are decided from.
pub struct Inputs<'a> {
    pub profile: &'a Profile,
    pub goals: &'a Goals,
    pub portrait: &'a [Sentence],
    pub choices: &'a BTreeMap<String, Choice>,
    pub changes: &'a [Change],
    pub accuracy: Option<Accuracy>,
    /// Skills the person said are not theirs (lower case).
    pub dismissed: &'a [String],
}

/// How many questions at most, by what the person said of the portrait.
pub const fn limit(accuracy: Option<Accuracy>) -> usize {
    match accuracy {
        Some(Accuracy::Yes) => MAX_QUESTIONS_ACCURATE,
        Some(Accuracy::No) => MAX_QUESTIONS_INACCURATE,
        Some(Accuracy::Some) | None => MAX_QUESTIONS,
    }
}

/// Title words that mean leading people, in the languages the app speaks.
const LEADING: &[&str] = &[
    "lead",
    "leader",
    "manager",
    "head",
    "director",
    "cto",
    "vp",
    "líder",
    "lider",
    "gerente",
    "gestor",
    "diretor",
    "directeur",
    "responsable",
    "jefe",
    "coordenador",
    "coordinador",
];

/// Whether `title` names someone who leads people.
pub fn leads(title: &str) -> bool {
    title
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .any(|word| LEADING.contains(&word.as_str()))
}

fn describe(work: &Work) -> String {
    match (work.position.as_deref(), work.name.as_deref()) {
        (Some(title), Some(company)) => format!("{title} at {company}"),
        (Some(x), None) | (None, Some(x)) => x.to_owned(),
        (None, None) => "a position".to_owned(),
    }
}

/// Whether `change` is a fact about `work` (the position at `index`).
fn about_role(change: &Change, work: &Work, index: usize) -> bool {
    change.field == Field::RoleFact
        && match &change.role_key {
            Some(key) => *key == grill::role_key(work),
            None => change.position.and_then(|p| usize::try_from(p).ok()) == Some(index),
        }
}

fn has_digit(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

/// The words of a position, for finding the technologies it names.
fn words_of(work: &Work) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(summary) = work.summary.as_deref() {
        parts.push(summary);
    }
    parts.extend(work.highlights.iter().map(String::as_str));
    parts.extend(work.facts.iter().map(|f| f.text.as_str()));
    parts.join(" ")
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '/' | '(' | ')' | '"' | ':'))
}

fn bare(token: &str) -> String {
    token
        .trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#' | '.')))
        .trim_end_matches('.')
        .to_owned()
}

/// Whether `work` names the technology `name`.
fn names(work: &Work, name: &str) -> bool {
    tokens(&words_of(work)).any(|token| tech::same_skill(&bare(token), name))
}

/// Technologies the positions name that are not skills, the most roles
/// first: (name as written, in how many positions).
fn found_in_roles(i: &Inputs<'_>) -> Vec<(String, usize)> {
    let mut found: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for work in &i.profile.work {
        let here: BTreeSet<String> = tokens(&words_of(work))
            .filter(|token| tech::is_name(token))
            .map(bare)
            .collect();
        let mut seen = BTreeSet::new();
        for name in here {
            let key = tech::canonical(&name);
            if seen.insert(key.clone()) {
                found.entry(key).or_insert((name, 0)).1 += 1;
            }
        }
    }
    let pending: Vec<&str> = i
        .changes
        .iter()
        .filter(|c| c.field == Field::Skill)
        .map(|c| c.value.as_str())
        .collect();
    let mut found: Vec<(String, usize)> = found
        .into_values()
        .filter(|(name, _)| {
            !i.profile
                .skills
                .iter()
                .any(|s| tech::same_skill(&s.name, name))
                && !pending.iter().any(|p| tech::same_skill(p, name))
                && !i.dismissed.contains(&name.to_lowercase())
        })
        .collect();
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found.truncate(MAX_FOUND);
    found
}

fn area_phrase(area: Area) -> &'static str {
    match area {
        Area::Context => "what the company sells and to whom",
        Area::YourPart => "what they did themselves, as against the team",
        Area::StackScale => "the tools, data size or traffic",
        Area::Results => "results, with numbers",
        Area::HardProblems => "the hardest problem and what they did about it",
        Area::People => "leading, mentoring or hiring",
        Area::Stories => "a story they would tell in an interview",
    }
}

/// The gaps, the most useful first. Pure: the same inputs, the same gaps.
pub fn gaps(i: &Inputs<'_>) -> Vec<Gap> {
    let mut out: Vec<(u32, Gap)> = Vec::new();
    let gap = |id: String,
               kind: GapKind,
               position: Option<usize>,
               area: Option<Area>,
               basis: String,
               names: Vec<String>| Gap {
        id,
        kind,
        position: position.and_then(|p| u32::try_from(p).ok()),
        area,
        basis,
        names,
    };

    // 1. Assumptions to settle: left for later, or (after "Not really")
    // marked wrong.
    for (n, sentence) in i.portrait.iter().enumerate() {
        if sentence.kind != SentenceKind::Assumed {
            continue;
        }
        let rank = u32::try_from(n).unwrap_or(0);
        let reason = sentence
            .why
            .clone()
            .unwrap_or_else(|| "it is usual for the role".to_owned());
        let (rank, basis) = match i.choices.get(&sentence.id) {
            Some(Choice::Later) => (
                rank,
                format!(
                    "The person chose \"ask me later\" on this assumption of mine: \"{}\". \
                     I assumed it because: {reason}",
                    sentence.text
                ),
            ),
            Some(Choice::Wrong) if i.accuracy == Some(Accuracy::No) => (
                20 + rank,
                format!(
                    "The person said this assumption of mine is wrong: \"{}\". \
                     I assumed it because: {reason}. What is true instead is not known.",
                    sentence.text
                ),
            ),
            _ => continue,
        };
        out.push((
            rank,
            gap(
                format!("assumed:{}", sentence.id),
                GapKind::Assumed,
                sentence.position.and_then(|p| usize::try_from(p).ok()),
                Some(sentence.topic),
                basis,
                Vec::new(),
            ),
        ));
    }

    // The recent positions.
    let recent = i.profile.work.len().min(RECENT);
    for (index, work) in i.profile.work.iter().enumerate().take(recent) {
        let told: Vec<&Change> = i
            .changes
            .iter()
            .filter(|c| about_role(c, work, index))
            .collect();
        let kept_or_told = |area: Area| {
            work.facts.iter().any(|f| f.area == area) || told.iter().any(|c| c.area == area)
        };
        let what = describe(work);

        let numbered = strength::has_numbered_result(work)
            || told
                .iter()
                .any(|c| c.area == Area::Results && has_digit(&c.value));
        if !numbered {
            let has_results = work.highlights.iter().any(|h| !h.trim().is_empty())
                || work.facts.iter().any(|f| f.area == Area::Results);
            let basis = if has_results {
                format!("{what} states results, but none with a number.")
            } else {
                format!("{what} states no results.")
            };
            let rank = if index == 0 { 100 } else { 130 + index as u32 };
            out.push((
                rank,
                gap(
                    format!("results:{index}"),
                    GapKind::Results,
                    Some(index),
                    Some(Area::Results),
                    basis,
                    Vec::new(),
                ),
            ));
        }

        let team = work.position.as_deref().is_some_and(leads) && !kept_or_told(Area::People);
        if team {
            out.push((
                120 + index as u32,
                gap(
                    format!("team:{index}"),
                    GapKind::TeamSize,
                    Some(index),
                    Some(Area::People),
                    format!(
                        "The title is {}, which usually means leading people. \
                         The profile does not say how many.",
                        work.position.as_deref().unwrap_or_default()
                    ),
                    Vec::new(),
                ),
            ));
        }

        for (n, area) in AREAS.into_iter().chain([Area::People]).enumerate() {
            // People is the team size question for a lead; the last of the
            // areas for anyone else.
            if (area == Area::People && team) || kept_or_told(area) {
                continue;
            }
            out.push((
                200 + 10 * index as u32 + n as u32,
                gap(
                    format!("area:{index}:{}", area.as_str()),
                    GapKind::Area,
                    Some(index),
                    Some(area),
                    format!("Nothing is kept about {} for {what}.", area_phrase(area)),
                    Vec::new(),
                ),
            ));
        }
    }

    // 3. Empty skills.
    let told_skills = i.changes.iter().any(|c| c.field == Field::Skill);
    if i.profile.skills.is_empty() && !told_skills {
        out.push((
            110,
            gap(
                "skills".to_owned(),
                GapKind::Skills,
                None,
                None,
                "The profile lists no skills.".to_owned(),
                Vec::new(),
            ),
        ));
    }

    // Technologies the roles name that the skills list lacks.
    let found = found_in_roles(i);
    if !found.is_empty() {
        let list: Vec<String> = found.into_iter().map(|(name, _)| name).collect();
        out.push((
            140,
            gap(
                "skills_found".to_owned(),
                GapKind::SkillsFound,
                None,
                None,
                format!(
                    "The roles mention {}, but they are not in the skills list.",
                    list.join(", ")
                ),
                list,
            ),
        ));
    }

    // A skill the person told, which none of their roles mention.
    let mut seen = BTreeSet::new();
    for change in i.changes.iter().filter(|c| c.field == Field::Skill) {
        let key = tech::canonical(&change.value);
        if i.profile.work.iter().any(|w| names(w, &change.value)) || !seen.insert(key.clone()) {
            continue;
        }
        out.push((
            150 + u32::try_from(seen.len()).unwrap_or(0),
            gap(
                format!("claim:{key}"),
                GapKind::Claim,
                None,
                None,
                format!(
                    "The person told me they use {}, but none of their roles mention it.",
                    change.value
                ),
                vec![change.value.clone()],
            ),
        ));
    }

    // The basics.
    let told_field = |field: Field| i.changes.iter().any(|c| c.field == field);
    let blank = |value: &Option<String>| value.as_deref().is_none_or(|v| v.trim().is_empty());
    let basics = &i.profile.basics;
    for (rank, id, kind, empty, field, basis) in [
        (
            160,
            "summary",
            GapKind::Summary,
            blank(&basics.summary),
            Field::Summary,
            "The profile has no summary.",
        ),
        (
            161,
            "headline",
            GapKind::Headline,
            blank(&basics.label),
            Field::Headline,
            "The profile has no headline.",
        ),
        (
            162,
            "background",
            GapKind::Background,
            blank(&basics.background),
            Field::Background,
            "The profile says nothing of earlier careers or other fields.",
        ),
    ] {
        if empty && !told_field(field) {
            out.push((
                rank,
                gap(
                    id.to_owned(),
                    kind,
                    None,
                    None,
                    basis.to_owned(),
                    Vec::new(),
                ),
            ));
        }
    }

    // Goals.
    if i.goals.roles.is_empty() && !told_field(Field::GoalRole) {
        out.push((
            170,
            gap(
                "goals".to_owned(),
                GapKind::Goals,
                None,
                None,
                "The person has not said which roles they want next.".to_owned(),
                Vec::new(),
            ),
        ));
    }

    out.sort_by_key(|(rank, _)| *rank);
    out.into_iter().map(|(_, gap)| gap).collect()
}

/// What answering `gap` would add to the profile's strength, in points: the
/// strength with the gap filled less the strength now. Both count what the
/// person has already told, as if it were saved. Zero for a gap that is not
/// a part of the score (a team size, an area).
pub fn gain(gap: &Gap, profile: &Profile, goals: &Goals, now: DateTime<Utc>) -> u8 {
    let before = strength::compute(profile, goals).score;
    let mut after = profile.clone();
    let mut goals_after = goals.clone();
    match gap.kind {
        GapKind::Results => {
            if let Some(work) = gap
                .position
                .and_then(|p| usize::try_from(p).ok())
                .and_then(|p| after.work.get_mut(p))
            {
                work.facts.push(ProfileFact {
                    id: String::new(),
                    text: "1".to_owned(),
                    quote: "1".to_owned(),
                    area: Area::Results,
                    kept_at: now,
                    status: FactStatus::Told,
                    basis: None,
                });
            }
        }
        // A first answer names a few skills.
        GapKind::Skills => {
            for n in 0..3 {
                after.skills.push(Skill {
                    name: format!("skill {n}"),
                    level: None,
                });
            }
        }
        GapKind::Summary => after.basics.summary = Some("x".to_owned()),
        GapKind::Headline => after.basics.label = Some("x".to_owned()),
        GapKind::Goals => goals_after.roles.push("x".to_owned()),
        _ => {}
    }
    strength::compute(&after, &goals_after)
        .score
        .saturating_sub(before)
}
