//! How strong a profile is: a score out of 100 computed in code from the
//! profile and the goals, never by a model.
//!
//! The weights add up to 100 and say what agents can use: a headline and a
//! summary, achievements with results on the most recent roles, confirmed
//! skills, education, languages, a link, the positions themselves and the
//! goals. Nothing counts that the person would rather leave out; the score
//! only ever rises with what they wrote or kept.

use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    goals::Goals,
    matching,
    schema::{Area, Profile, Work},
};

/// A profile this strong or more is strong.
pub const STRONG: u8 = 80;
/// How many of the most recent positions count for achievements.
pub const RECENT_ROLES: usize = 3;
/// Skills needed for the full skills weight.
pub const FULL_SKILLS: usize = 5;

/// What the score is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrengthKey {
    Headline,
    Summary,
    Achievements,
    Skills,
    Education,
    Languages,
    Links,
    Positions,
    Goals,
}

impl StrengthKey {
    /// Every part, in the order the checklist shows them.
    pub const ALL: [Self; 9] = [
        Self::Headline,
        Self::Summary,
        Self::Achievements,
        Self::Skills,
        Self::Education,
        Self::Languages,
        Self::Links,
        Self::Positions,
        Self::Goals,
    ];

    /// Its weight in the score.
    pub const fn weight(self) -> u32 {
        match self {
            Self::Headline => 6,
            Self::Summary => 8,
            Self::Achievements => 24,
            Self::Skills => 12,
            Self::Education | Self::Languages => 6,
            Self::Links => 4,
            Self::Positions => 16,
            Self::Goals => 18,
        }
    }
}

/// Where a part is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrengthState {
    /// Full weight.
    Ok,
    /// Some of the weight.
    Partial,
    /// None of it.
    Missing,
}

/// One part of the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// Which part.
    pub key: StrengthKey,
    /// Where it is.
    pub state: StrengthState,
    /// Points earned, out of [`StrengthKey::weight`].
    pub earned: u32,
    /// What there is: skills listed, roles with results, positions…
    pub have: u32,
    /// What there should be for the full weight (0 when it is a yes or no).
    pub of: u32,
}

/// The score and its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strength {
    /// 0 to 100.
    pub score: u8,
    /// The parts, in checklist order.
    pub items: Vec<Item>,
}

impl Strength {
    /// At [`STRONG`] or more.
    pub fn is_strong(&self) -> bool {
        self.score >= STRONG
    }

    /// The part `key`.
    pub fn item(&self, key: StrengthKey) -> Option<&Item> {
        self.items.iter().find(|item| item.key == key)
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty())
}

/// Whether a highlight states a result: it holds a number or a percentage.
fn has_number(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

/// What a position states as results: its highlights, and the facts the
/// person kept about its results ("Tell me about you", #44).
fn results_of(work: &Work) -> Vec<&str> {
    work.highlights
        .iter()
        .map(String::as_str)
        .chain(
            work.facts
                .iter()
                .filter(|f| f.area == Area::Results)
                .map(|f| f.text.as_str()),
        )
        .filter(|h| !h.trim().is_empty())
        .collect()
}

/// Whether a position states a result with a number or a percentage.
pub fn has_numbered_result(work: &Work) -> bool {
    results_of(work).iter().any(|h| has_number(h))
}

/// Points of one recent role, out of 8: a result is 6, one with a number
/// or a percentage another 2.
fn role_points(work: &Work) -> u32 {
    let real = results_of(work);
    if real.is_empty() {
        0
    } else if real.iter().any(|h| has_number(h)) {
        8
    } else {
        6
    }
}

fn item(key: StrengthKey, earned: u32, have: u32, of: u32) -> Item {
    let state = if earned >= key.weight() {
        StrengthState::Ok
    } else if earned == 0 {
        StrengthState::Missing
    } else {
        StrengthState::Partial
    };
    Item {
        key,
        state,
        earned: earned.min(key.weight()),
        have,
        of,
    }
}

fn yes(key: StrengthKey, there: bool) -> Item {
    item(
        key,
        if there { key.weight() } else { 0 },
        u32::from(there),
        0,
    )
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The strength of `profile` with `goals`.
pub fn compute(profile: &Profile, goals: &Goals) -> Strength {
    let recent = &profile.work[..profile.work.len().min(RECENT_ROLES)];
    // A person with one position can still earn the full weight: the 24
    // points are shared out over the recent roles there are.
    let achievements = if recent.is_empty() {
        0
    } else {
        let points: u32 = recent.iter().map(role_points).sum();
        (points * StrengthKey::Achievements.weight()).div_ceil(8 * count(recent.len()))
    };
    let with_results = count(recent.iter().filter(|w| !results_of(w).is_empty()).count());
    let skills = count(profile.skills.len());
    let dated = profile
        .work
        .iter()
        .filter(|w| present(w.start_date.as_deref()))
        .count();
    let positions = if dated >= 3 {
        16
    } else if profile.work.is_empty() {
        0
    } else {
        8
    };
    let (answered, total) = matching::answered(goals);
    let items = vec![
        yes(
            StrengthKey::Headline,
            present(profile.basics.label.as_deref()),
        ),
        yes(
            StrengthKey::Summary,
            present(profile.basics.summary.as_deref()),
        ),
        item(
            StrengthKey::Achievements,
            achievements,
            with_results,
            count(recent.len()),
        ),
        item(
            StrengthKey::Skills,
            (skills.min(count(FULL_SKILLS)) * StrengthKey::Skills.weight()) / count(FULL_SKILLS),
            skills,
            count(FULL_SKILLS),
        ),
        yes(StrengthKey::Education, !profile.education.is_empty()),
        yes(StrengthKey::Languages, !profile.languages.is_empty()),
        yes(
            StrengthKey::Links,
            !profile.basics.profiles.is_empty() || present(profile.basics.url.as_deref()),
        ),
        item(
            StrengthKey::Positions,
            positions,
            count(profile.work.len()),
            3,
        ),
        item(
            StrengthKey::Goals,
            (answered * StrengthKey::Goals.weight()) / total.max(1),
            answered,
            total,
        ),
    ];
    let score: u32 = items.iter().map(|i| i.earned).sum();
    Strength {
        score: u8::try_from(score.min(100)).unwrap_or(100),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Basics, Education, Language, Link, Skill, Work};

    fn role(highlights: &[&str]) -> Work {
        Work {
            name: Some("Example Org".into()),
            position: Some("Engineer".into()),
            start_date: Some("2020-01".into()),
            highlights: highlights.iter().map(|h| (*h).to_owned()).collect(),
            ..Work::default()
        }
    }

    fn goals_answered(n: usize) -> Goals {
        let mut goals = Goals::default();
        if n >= 1 {
            goals.roles = vec!["Backend engineer".into()];
        }
        goals
    }

    #[test]
    fn the_weights_add_up_to_one_hundred() {
        assert_eq!(
            StrengthKey::ALL.iter().map(|k| k.weight()).sum::<u32>(),
            100
        );
    }

    #[test]
    fn an_empty_profile_scores_nothing() {
        let strength = compute(&Profile::default(), &Goals::default());
        assert_eq!(strength.score, 0);
        assert!(
            strength
                .items
                .iter()
                .all(|i| i.state == StrengthState::Missing)
        );
        assert!(!strength.is_strong());
    }

    #[test]
    fn each_part_scores_by_its_table() {
        let score = |profile: &Profile| compute(profile, &Goals::default()).score;
        let mut profile = Profile::default();
        profile.basics.label = Some("Backend engineer".into());
        assert_eq!(score(&profile), 6, "headline");
        profile.basics.summary = Some("I build calm backends.".into());
        assert_eq!(score(&profile), 14, "+ summary");
        profile.education.push(Education {
            institution: "Sample University".into(),
            ..Education::default()
        });
        profile.languages.push(Language {
            language: "English".into(),
            fluency: None,
        });
        assert_eq!(score(&profile), 26, "+ education + languages");
        profile.basics.profiles.push(Link {
            network: None,
            url: "https://github.com/demo".into(),
        });
        assert_eq!(score(&profile), 30, "+ a link");
        profile.work.push(role(&[]));
        assert_eq!(score(&profile), 38, "+ one dated position");
        profile.work.push(role(&[]));
        profile.work.push(role(&[]));
        assert_eq!(score(&profile), 46, "three dated positions = 16");
        for name in ["Rust", "SQL"] {
            profile.skills.push(Skill {
                name: name.into(),
                level: None,
            });
        }
        assert_eq!(score(&profile), 50, "two of five skills = 4 of 12");
        for name in ["Go", "Linux", "Git"] {
            profile.skills.push(Skill {
                name: name.into(),
                level: None,
            });
        }
        assert_eq!(score(&profile), 58, "five skills = 12");
    }

    #[test]
    fn achievements_count_results_on_the_three_most_recent_roles() {
        let mut profile = Profile {
            work: vec![role(&[]), role(&[]), role(&[]), role(&["Old win, 90%"])],
            ..Profile::default()
        };
        let of = |profile: &Profile| {
            compute(profile, &Goals::default())
                .item(StrengthKey::Achievements)
                .copied()
                .unwrap()
        };
        assert_eq!(of(&profile).earned, 0, "the fourth role does not count");
        profile.work[0].highlights = vec!["Rebuilt billing".into()];
        let one = of(&profile);
        assert_eq!(
            (one.earned, one.have, one.of),
            (6, 1, 3),
            "6 of 8 of a third"
        );
        assert_eq!(one.state, StrengthState::Partial);
        profile.work[0].highlights = vec!["Cut runs from 6 hours to 40 minutes".into()];
        assert_eq!(of(&profile).earned, 8, "a number is worth 2 more");
        for work in &mut profile.work[..3] {
            work.highlights = vec!["Cut runs by 40%".into()];
        }
        let full = of(&profile);
        assert_eq!((full.earned, full.state), (24, StrengthState::Ok));
    }

    #[test]
    fn a_kept_fact_about_results_counts_like_a_highlight() {
        use crate::schema::{FactStatus, ProfileFact};
        let fact = |text: &str, area: Area| ProfileFact {
            id: "f".into(),
            text: text.into(),
            quote: text.into(),
            area,
            kept_at: chrono::Utc::now(),
            status: FactStatus::Told,
            basis: None,
        };
        let mut work = role(&[]);
        let of = |work: &Work| {
            compute(
                &Profile {
                    work: vec![work.clone()],
                    ..Profile::default()
                },
                &Goals::default(),
            )
            .item(StrengthKey::Achievements)
            .copied()
            .unwrap()
            .earned
        };
        assert_eq!(of(&work), 0);
        work.facts.push(fact("Led a small team", Area::People));
        assert_eq!(of(&work), 0, "only results count");
        work.facts.push(fact("Rebuilt billing", Area::Results));
        assert_eq!(of(&work), 18, "a result is 6 of 8");
        work.facts.push(fact("Cut runs by 40%", Area::Results));
        assert_eq!(of(&work), 24, "a number is worth 2 more");
        assert!(has_numbered_result(&work));
    }

    #[test]
    fn one_position_can_earn_the_whole_achievements_weight() {
        let profile = Profile {
            work: vec![role(&["Cut runs by 40%"])],
            ..Profile::default()
        };
        let item = *compute(&profile, &Goals::default())
            .item(StrengthKey::Achievements)
            .unwrap();
        assert_eq!(item.earned, 24);
    }

    #[test]
    fn goals_count_in_proportion() {
        let none = compute(&Profile::default(), &goals_answered(0)).score;
        let some = compute(&Profile::default(), &goals_answered(1)).score;
        assert_eq!(none, 0);
        assert_eq!(some, 1, "1 of 11 answered = 18 / 11 rounded down");
        let everything = Goals {
            roles: vec!["Backend engineer".into()],
            relocate: Some(crate::goals::Relocate::No),
            ..Goals::default()
        };
        assert_eq!(compute(&Profile::default(), &everything).score, 3);
    }

    #[test]
    fn a_whole_profile_is_strong() {
        let mut profile = Profile {
            basics: Basics {
                label: Some("Backend engineer".into()),
                summary: Some("I build calm backends.".into()),
                url: Some("https://demo.example.com".into()),
                ..Basics::default()
            },
            work: vec![
                role(&["Cut runs by 40%"]),
                role(&["Shipped in 3 weeks"]),
                role(&["Led 4 people"]),
            ],
            skills: ["Rust", "SQL", "Go", "Linux", "Git"]
                .iter()
                .map(|n| Skill {
                    name: (*n).into(),
                    level: None,
                })
                .collect(),
            ..Profile::default()
        };
        profile.education.push(Education {
            institution: "Sample University".into(),
            ..Education::default()
        });
        profile.languages.push(Language {
            language: "English".into(),
            fluency: None,
        });
        let strength = compute(&profile, &Goals::default());
        assert_eq!(strength.score, 82, "100 less the goals' 18");
        assert!(strength.is_strong());
    }
}
