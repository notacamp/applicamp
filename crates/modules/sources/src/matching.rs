//! Choosing which jobs to score, without the model.
//!
//! 1. **Hard limits** from the goals' structured answers, on the job's
//!    structured fields only (never its prose): too old, expired (a board
//!    whose terms drop expired jobs), or clearly in the wrong place or the
//!    wrong way of working (Ashby's `isRemote` / `workplaceType` and
//!    Lever's `workplaceType` for someone who works remote only; a job
//!    board's structured regions that exclude the country they live in,
//!    for someone who works remote only or would not move). Anything
//!    unclear goes to the model, whose facts the requirement rules
//!    ([`crate::requirements`]) decide on.
//! 2. **Pre-rank** by how much of a wanted role's words the title has, how
//!    recent the job is, and whether its structured location names
//!    somewhere they can work from (remote, Europe, their places, their
//!    country); the top [`TOP`] are scored.

use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};

use crate::{
    job::{Job, WorkMode},
    places, regions,
    requirements::{Floor, Reach},
};

/// Jobs scored per search, at most.
pub const TOP: usize = 40;
/// Jobs older than this are left out.
pub const MAX_AGE: TimeDelta = TimeDelta::days(30);

/// Skills that drive the stack queries, the pre-rank and the fit label.
pub const TOP_SKILLS: usize = 3;

/// The person's top skills: the first [`TOP_SKILLS`] non-blank entries, in
/// their order, without repeats (ignoring case).
pub fn top_skills(skills: &[String]) -> Vec<&str> {
    let mut top: Vec<&str> = Vec::new();
    for skill in skills.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if top.len() == TOP_SKILLS {
            break;
        }
        if !top.iter().any(|t| t.eq_ignore_ascii_case(skill)) {
            top.push(skill);
        }
    }
    top
}

/// A skill's words (`Ruby on Rails` is `ruby` + `rails`).
pub fn skill_words(skills: &[&str]) -> Vec<HashSet<String>> {
    skills
        .iter()
        .map(|s| words(s))
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether a text's `words` name the skill `skill` (all its words).
pub fn names_skill(text_words: &HashSet<String>, skill: &HashSet<String>) -> bool {
    !skill.is_empty() && skill.iter().all(|w| text_words.contains(w))
}

/// The words of a job's title and the start of its description.
pub fn job_words(title: &str, description: &str) -> HashSet<String> {
    let mut all = words(title);
    all.extend(words(
        &description
            .chars()
            .take(DESCRIPTION_CHARS)
            .collect::<String>(),
    ));
    all
}

/// How much of a description names a skill.
const DESCRIPTION_CHARS: usize = 4_000;

/// What the pre-rank adds when every top skill is named.
const SKILL_FIT: f64 = 0.3;

/// What a search matches against: the person's profile and goals, as the
/// profile module gives them (through the server).
#[derive(Debug, Clone, Default)]
pub struct MatchInput {
    /// The profile's version (when it last changed), for the score cache.
    pub profile_version: String,
    /// The goals' version, for the score cache.
    pub goals_version: i32,
    /// A compact summary of the profile, for the model (no name or contact).
    pub profile: serde_json::Value,
    /// The goals, for the model.
    pub goals: serde_json::Value,
    /// The roles they want.
    pub roles: Vec<String>,
    /// The roles they have had, for ranking when they named none.
    pub titles: Vec<String>,
    /// How they want to work (none answered: any).
    pub work_modes: Vec<WorkMode>,
    /// Where they can work from.
    pub locations: Vec<String>,
    /// They said they would not move.
    pub will_not_relocate: bool,
    /// Where they live: the first of `locations`, else the profile's.
    pub home: Option<String>,
    /// How far they would move; `None` when not answered.
    pub relocate: Option<Reach>,
    /// The languages they work in, ISO 639-1 (`en`); none known: any.
    pub languages: Vec<String>,
    /// The least pay they take.
    pub cash_floor: Option<Floor>,
    /// The skills on their profile, in their own order (at most 40).
    pub skills: Vec<String>,
}

impl MatchInput {
    /// They work remote only.
    pub fn remote_only(&self) -> bool {
        !self.work_modes.is_empty() && self.work_modes.iter().all(|m| *m == WorkMode::Remote)
    }
}

/// Why a job was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    /// Older than [`MAX_AGE`].
    TooOld,
    /// Already in their pipeline, or dismissed.
    Known,
    /// Not the way they want to work.
    WorkMode,
    /// On site somewhere they cannot be.
    Place,
    /// They work remote only, and the board's own field says it is not
    /// remote.
    NotRemote,
    /// Past the expiry its board gives.
    Expired,
    /// Its board's regions exclude the country they live in, and they work
    /// remote only or would not move.
    Region,
}

/// Whether `job` breaks a hard limit of `input`, and which; `known` are the
/// `(source, source_ref)` already in their pipeline.
pub fn hard_limit(
    job: &Job,
    input: &MatchInput,
    known: &HashSet<(String, String)>,
    now: DateTime<Utc>,
) -> Option<Out> {
    if known.contains(&job.key()) {
        return Some(Out::Known);
    }
    if job.posted_at.is_some_and(|at| now - at > MAX_AGE) {
        return Some(Out::TooOld);
    }
    if input.remote_only() && job.board_remote == Some(false) {
        return Some(Out::NotRemote);
    }
    if let Some(board) = &job.board {
        if board.expires_at.is_some_and(|at| at < now) {
            return Some(Out::Expired);
        }
        let bound = input.remote_only() || input.relocate == Some(Reach::No);
        if bound
            && let Some(country) = input.home.as_deref().and_then(regions::country_of)
            && regions::open_to(&board.regions, country) == Some(false)
        {
            return Some(Out::Region);
        }
    }
    let mode = job.work_mode?;
    // Remote suits anyone; an office day suits only who asked for it.
    if mode != WorkMode::Remote && !input.work_modes.is_empty() && !input.work_modes.contains(&mode)
    {
        return Some(Out::WorkMode);
    }
    if mode != WorkMode::Remote
        && input.will_not_relocate
        && !input.locations.is_empty()
        && let Some(location) = job.location.as_deref()
    {
        let location = location.to_lowercase();
        let reachable = input.locations.iter().any(|place| {
            let place = place.to_lowercase();
            location.contains(&place) || place.contains(&location)
        });
        if !reachable {
            return Some(Out::Place);
        }
    }
    None
}

/// Words that say nothing about a role.
const NOISE: [&str; 24] = [
    "a", "an", "and", "at", "de", "der", "des", "di", "du", "e", "en", "et", "for", "in", "la",
    "le", "of", "on", "or", "the", "to", "with", "y", "und",
];

/// The words of `text`, lower-cased, without punctuation or noise.
pub fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() > 1 && !NOISE.contains(&w.as_str()))
        .collect()
}

/// How well `title` fits the best of `roles`: the share of that role's
/// words the title has, 0 to 1.
pub fn title_fit(title: &str, roles: &[HashSet<String>]) -> f64 {
    let title = words(title);
    roles
        .iter()
        .filter(|role| !role.is_empty())
        // Counts of a few words: exact as f64.
        .map(|role| role.intersection(&title).count() as f64 / role.len() as f64)
        .fold(0.0, f64::max)
}

/// Newer is better: 0.2 for today, down to 0 at [`MAX_AGE`] (and for an
/// undated job, 0.1).
fn freshness(posted_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let Some(at) = posted_at else {
        return 0.1;
    };
    // Hours in a month: exact as f64.
    let age = (now - at).num_hours().max(0) as f64 / 24.0;
    let max = MAX_AGE.num_days() as f64;
    0.2 * (1.0 - (age / max).min(1.0))
}

/// Words in a structured location that say it may be open to someone in
/// Europe.
const OPEN_WORDS: [&str; 5] = ["remote", "europe", "emea", "eu", "anywhere"];

/// What the pre-rank adds for a location that may suit them.
const PLACE_FIT: f64 = 0.15;

/// [`PLACE_FIT`] when `location` (the board's field, never the prose) names
/// remote, Europe, EMEA or the EU, one of their places, or the country
/// they live in; else 0. A preference only: the rules decide.
fn place_fit(location: Option<&str>, input: &MatchInput) -> f64 {
    let Some(location) = location else {
        return 0.0;
    };
    let named = places::words(location);
    let open = named.iter().any(|w| OPEN_WORDS.contains(&w.as_str()));
    let theirs = input
        .locations
        .iter()
        .chain(&input.home)
        .any(|place| places::same_place(location, place));
    let home_countries: Vec<&str> = input
        .home
        .as_deref()
        .map(places::countries_in)
        .unwrap_or_default();
    let country = places::countries_in(location)
        .iter()
        .any(|c| home_countries.contains(c));
    if open || theirs || country {
        PLACE_FIT
    } else {
        0.0
    }
}

/// [`SKILL_FIT`] times the share of `top` skills `job` names; 0 without any.
fn skill_fit(job: &Job, top: &[HashSet<String>]) -> f64 {
    if top.is_empty() {
        return 0.0;
    }
    let text = job_words(&job.title, &job.description);
    let hits = top.iter().filter(|s| names_skill(&text, s)).count();
    // Counts of at most three: exact as f64.
    SKILL_FIT * hits as f64 / top.len() as f64
}

/// The best [`TOP`] of `jobs` for `input`, best first.
pub fn pre_rank(jobs: Vec<Job>, input: &MatchInput, now: DateTime<Utc>) -> Vec<Job> {
    let wanted = if input.roles.is_empty() {
        &input.titles
    } else {
        &input.roles
    };
    let roles: Vec<HashSet<String>> = wanted.iter().map(|r| words(r)).collect();
    let top = skill_words(&top_skills(&input.skills));
    let mut ranked: Vec<(f64, Job)> = jobs
        .into_iter()
        .map(|job| {
            (
                title_fit(&job.title, &roles)
                    + freshness(job.posted_at, now)
                    + place_fit(job.location.as_deref(), input)
                    + skill_fit(&job, &top),
                job,
            )
        })
        .collect();
    ranked.sort_by(|(a, ja), (b, jb)| {
        b.total_cmp(a)
            .then_with(|| jb.posted_at.cmp(&ja.posted_at))
            .then_with(|| ja.source_ref.cmp(&jb.source_ref))
    });
    ranked.into_iter().take(TOP).map(|(_, job)| job).collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use crate::job::Source;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
    }

    fn job(title: &str, location: Option<&str>, mode: Option<WorkMode>, days: i64) -> Job {
        Job {
            board: None,
            source: Source::Lever,
            source_ref: title.to_lowercase().replace(' ', "-"),
            title: title.into(),
            company: "Example Co".into(),
            location: location.map(Into::into),
            work_mode: mode,
            board_remote: mode.map(|m| m == WorkMode::Remote),
            url: "https://example.com/job".into(),
            posted_at: Some(now() - TimeDelta::days(days)),
            salary_text: None,
            description: String::new(),
        }
    }

    fn remote_in_portugal() -> MatchInput {
        MatchInput {
            roles: vec!["Staff engineer".into(), "Founding engineer".into()],
            work_modes: vec![WorkMode::Remote, WorkMode::Hybrid],
            locations: vec!["Lisbon".into(), "Portugal".into()],
            will_not_relocate: true,
            ..MatchInput::default()
        }
    }

    #[test]
    fn hard_limits_use_structured_fields_only() {
        let input = remote_in_portugal();
        let known: HashSet<_> = [("lever".to_owned(), "known-one".to_owned())].into();
        let out = |j: &Job| hard_limit(j, &input, &known, now());
        assert_eq!(out(&job("Known one", None, None, 1)), Some(Out::Known));
        assert_eq!(out(&job("Old", None, None, 31)), Some(Out::TooOld));
        assert_eq!(
            out(&job("Desk", Some("Berlin"), Some(WorkMode::Onsite), 1)),
            Some(Out::WorkMode)
        );
        assert_eq!(
            out(&job(
                "Hybrid",
                Some("Berlin, Germany"),
                Some(WorkMode::Hybrid),
                1
            )),
            Some(Out::Place)
        );
        assert_eq!(
            out(&job(
                "Hybrid",
                Some("Lisbon, Portugal"),
                Some(WorkMode::Hybrid),
                1
            )),
            None
        );
        assert_eq!(
            out(&job("Remote", Some("Berlin"), Some(WorkMode::Remote), 1)),
            None
        );
        // No structured work mode: the model decides.
        assert_eq!(out(&job("Unclear", Some("Berlin"), None, 1)), None);
        // Remote only: the board's own "not remote" (Ashby's `isRemote:
        // false` without a workplace type) is enough.
        let remote_only = MatchInput {
            work_modes: vec![WorkMode::Remote],
            ..remote_in_portugal()
        };
        let not_remote = Job {
            board_remote: Some(false),
            ..job("Desk", Some("Lisbon"), None, 1)
        };
        assert_eq!(
            hard_limit(&not_remote, &remote_only, &known, now()),
            Some(Out::NotRemote)
        );
        assert_eq!(
            out(&not_remote),
            None,
            "someone who also works hybrid keeps it for the model"
        );
        let lever_hybrid = job("Hybrid", Some("Lisbon"), Some(WorkMode::Hybrid), 1);
        assert_eq!(
            hard_limit(&lever_hybrid, &remote_only, &known, now()),
            Some(Out::NotRemote)
        );
        // Willing to move: a place is no limit.
        let movable = MatchInput {
            will_not_relocate: false,
            ..remote_in_portugal()
        };
        assert_eq!(
            hard_limit(
                &job("Hybrid", Some("Berlin"), Some(WorkMode::Hybrid), 1),
                &movable,
                &known,
                now()
            ),
            None
        );
    }

    #[test]
    fn a_boards_regions_and_expiry_are_hard_limits() {
        use crate::job::BoardJob;
        let on = |regions: &[&str], expired: bool| Job {
            board: Some(BoardJob {
                slug: "himalayas".into(),
                regions: regions.iter().map(|r| (*r).to_owned()).collect(),
                expires_at: expired.then(|| now() - TimeDelta::days(1)),
                ..BoardJob::default()
            }),
            ..job("Staff Engineer", None, Some(WorkMode::Remote), 1)
        };
        let lisbon = MatchInput {
            work_modes: vec![WorkMode::Remote],
            home: Some("Lisbon".into()),
            ..remote_in_portugal()
        };
        let known = HashSet::new();
        let out = |j: &Job| hard_limit(j, &lisbon, &known, now());
        assert_eq!(out(&on(&["United States"], false)), Some(Out::Region));
        assert_eq!(out(&on(&["Germany"], false)), Some(Out::Region));
        assert_eq!(out(&on(&["Europe"], false)), None);
        assert_eq!(out(&on(&["Anywhere"], false)), None);
        assert_eq!(out(&on(&[], false)), None, "no regions: the model reads it");
        assert_eq!(
            out(&on(&["Atlantis"], false)),
            None,
            "not understood: the model"
        );
        assert_eq!(out(&on(&["Europe"], true)), Some(Out::Expired));
        // Someone who would move within the EU keeps a Germany-only job.
        let mover = MatchInput {
            work_modes: vec![WorkMode::Remote, WorkMode::Hybrid],
            relocate: Some(Reach::WithinEu),
            will_not_relocate: false,
            ..lisbon
        };
        assert_eq!(
            hard_limit(&on(&["Germany"], false), &mover, &known, now()),
            None
        );
    }

    #[test]
    fn titles_like_the_wanted_roles_come_first() {
        let jobs = vec![
            job("Accountant", None, None, 0),
            job("Senior Staff Engineer, Platform", None, None, 10),
            job("Engineer", None, None, 1),
            job("Founding Engineer", None, None, 20),
        ];
        let ranked: Vec<String> = pre_rank(jobs, &remote_in_portugal(), now())
            .into_iter()
            .map(|j| j.title)
            .collect();
        assert_eq!(
            ranked,
            [
                "Senior Staff Engineer, Platform",
                "Founding Engineer",
                "Engineer",
                "Accountant"
            ]
        );
    }

    #[test]
    fn locations_that_may_suit_them_come_first_among_equals() {
        let input = MatchInput {
            home: Some("Lisbon".into()),
            locations: vec!["Lisbon".into()],
            ..remote_in_portugal()
        };
        let jobs = vec![
            job("Staff Engineer", Some("San Francisco"), None, 1),
            job("Staff Engineer", Some("Remote (EU)"), None, 1),
            job("Staff Engineer", Some("Porto, Portugal"), None, 1),
            job("Staff Engineer", Some("Lisbon"), None, 1),
        ];
        let ranked: Vec<Option<String>> = pre_rank(jobs, &input, now())
            .into_iter()
            .map(|j| j.location)
            .collect();
        assert_eq!(
            ranked.last().cloned().flatten().as_deref(),
            Some("San Francisco")
        );
    }

    #[test]
    fn only_the_top_forty_are_kept() {
        let jobs = (0..100)
            .map(|i| job(&format!("Engineer {i}"), None, None, 1))
            .collect();
        assert_eq!(pre_rank(jobs, &remote_in_portugal(), now()).len(), TOP);
    }

    #[test]
    fn the_top_skills_are_the_first_three_in_their_order() {
        let skills: Vec<String> = [
            "Ruby on Rails",
            " ",
            "ruby on rails",
            "React",
            "PostgreSQL",
            "Go",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            top_skills(&skills),
            ["Ruby on Rails", "React", "PostgreSQL"]
        );
        assert!(top_skills(&[]).is_empty());
    }

    #[test]
    fn a_job_that_names_the_stack_ranks_above_a_newer_one_that_does_not() {
        let mut rails = job("Backend Engineer", None, None, 20);
        rails.description = "You will build our Ruby on Rails monolith.".into();
        let other = job("Backend Engineer Two", None, None, 1);
        let input = MatchInput {
            roles: vec!["Backend Engineer".into()],
            skills: vec!["Ruby on Rails".into()],
            ..MatchInput::default()
        };
        let ranked = pre_rank(vec![other.clone(), rails.clone()], &input, now());
        assert_eq!(ranked[0].source_ref, rails.source_ref);
        // No skills: nothing changes, the fresher one leads.
        let plain = MatchInput {
            skills: vec![],
            ..input
        };
        assert_eq!(
            pre_rank(vec![rails, other.clone()], &plain, now())[0].source_ref,
            other.source_ref
        );
    }
}
