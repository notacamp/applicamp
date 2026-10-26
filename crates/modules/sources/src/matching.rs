//! Choosing which jobs to score, without the model.
//!
//! 1. **Hard limits** from the goals' structured answers, on the job's
//!    structured fields only (never its prose): too old, or clearly in the
//!    wrong place or the wrong way of working. Anything unclear stays for
//!    the model.
//! 2. **Pre-rank** by how much of a wanted role's words the title has, and
//!    how recent the job is; the top [`TOP`] are scored.

use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};

use crate::job::{Job, WorkMode};

/// Jobs scored per search, at most.
pub const TOP: usize = 40;
/// Jobs older than this are left out.
pub const MAX_AGE: TimeDelta = TimeDelta::days(30);

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

/// The best [`TOP`] of `jobs` for `input`, best first.
pub fn pre_rank(jobs: Vec<Job>, input: &MatchInput, now: DateTime<Utc>) -> Vec<Job> {
    let wanted = if input.roles.is_empty() {
        &input.titles
    } else {
        &input.roles
    };
    let roles: Vec<HashSet<String>> = wanted.iter().map(|r| words(r)).collect();
    let mut ranked: Vec<(f64, Job)> = jobs
        .into_iter()
        .map(|job| {
            (
                title_fit(&job.title, &roles) + freshness(job.posted_at, now),
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
            source: Source::Lever,
            source_ref: title.to_lowercase().replace(' ', "-"),
            title: title.into(),
            company: "Example Co".into(),
            location: location.map(Into::into),
            work_mode: mode,
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
    fn only_the_top_forty_are_kept() {
        let jobs = (0..100)
            .map(|i| job(&format!("Engineer {i}"), None, None, 1))
            .collect();
        assert_eq!(pre_rank(jobs, &remote_in_portugal(), now()).len(), TOP);
    }
}
