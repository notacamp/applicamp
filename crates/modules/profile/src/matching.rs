//! What other modules may ask of a profile, through the ports the server
//! wires (modules never import each other): how far a person is (Today),
//! and what a job search matches against.
//!
//! The summary a search sends to the model is compact and leaves out who
//! the person is: no name, address, links or contact details.

use applicamp_kernel::Result;
use chrono::{DateTime, Datelike as _, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{goals::Goals, schema::Profile};

/// Questions of the goals interview; one more (equity) when there is a cash
/// floor.
pub const GOAL_QUESTIONS: u32 = 11;

/// What could make a profile stronger, the most useful first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthHint {
    /// No headline.
    AddHeadline,
    /// No positions.
    AddWork,
    /// Fewer than three skills.
    AddSkills,
}

impl StrengthHint {
    /// Lower-case name, as the API says it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddHeadline => "add_headline",
            Self::AddWork => "add_work",
            Self::AddSkills => "add_skills",
        }
    }
}

/// How far a person is with their profile and goals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// They have a profile with something in it.
    pub profile_done: bool,
    /// What would make it stronger.
    pub strength_hint: Option<StrengthHint>,
    /// Goals questions answered.
    pub goals_answered: u32,
    /// Goals questions there are.
    pub goals_total: u32,
}

/// What a search matches jobs against.
#[derive(Debug, Clone)]
pub struct Matching {
    /// The profile.
    pub profile: Profile,
    /// When the profile last changed: its version for the score cache.
    pub profile_updated_at: DateTime<Utc>,
    /// The goals.
    pub goals: Goals,
    /// The goals' version.
    pub goals_version: i32,
}

/// The questions `goals` answers, and how many there are.
pub fn answered(goals: &Goals) -> (u32, u32) {
    let answers = [
        !goals.roles.is_empty(),
        !goals.seniority.is_empty(),
        !goals.work_mode.is_empty(),
        !goals.locations.is_empty(),
        goals.relocate.is_some(),
        goals.cash_floor.is_some(),
        !goals.company_stage.is_empty(),
        !goals.avoid.is_empty(),
        !goals.must_have.is_empty(),
        !goals.languages_for_work.is_empty(),
        goals.autonomy.is_some(),
    ];
    let mut answered = u32::try_from(answers.iter().filter(|a| **a).count()).unwrap_or(0);
    let mut total = GOAL_QUESTIONS;
    if goals.cash_floor.is_some() {
        total += 1;
        if goals.equity_counts.is_some() {
            answered += 1;
        }
    }
    (answered, total)
}

/// What would make `profile` stronger, if anything.
pub fn strength_hint(profile: &Profile) -> Option<StrengthHint> {
    if profile.basics.label.is_none() {
        Some(StrengthHint::AddHeadline)
    } else if profile.work.is_empty() {
        Some(StrengthHint::AddWork)
    } else if profile.skills.len() < 3 {
        Some(StrengthHint::AddSkills)
    } else {
        None
    }
}

async fn profile_of(db: &PgPool, account: Uuid) -> Result<Option<(Profile, DateTime<Utc>)>> {
    let row: Option<(sqlx::types::Json<Profile>, DateTime<Utc>)> =
        sqlx::query_as("SELECT data, updated_at FROM profile.profiles WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(profile, at)| (profile.0, at)))
}

async fn goals_of(db: &PgPool, account: Uuid) -> Result<Option<(Goals, i32)>> {
    let row: Option<(sqlx::types::Json<Goals>, i32)> =
        sqlx::query_as("SELECT answers, version FROM profile.goals WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(goals, version)| (goals.0, version)))
}

/// How far `account` is.
pub async fn status(db: &PgPool, account: Uuid) -> Result<Status> {
    let profile = profile_of(db, account)
        .await?
        .map(|(p, _)| p)
        .filter(|p| !p.is_empty());
    let (goals_answered, goals_total) = goals_of(db, account)
        .await?
        .map_or((0, GOAL_QUESTIONS), |(goals, _)| answered(&goals));
    Ok(Status {
        profile_done: profile.is_some(),
        strength_hint: profile.as_ref().and_then(strength_hint),
        goals_answered,
        goals_total,
    })
}

/// What a search for `account` matches against: none until they have a
/// profile and have answered at least one goals question.
pub async fn matching(db: &PgPool, account: Uuid) -> Result<Option<Matching>> {
    let Some((profile, profile_updated_at)) = profile_of(db, account).await? else {
        return Ok(None);
    };
    let Some((goals, goals_version)) = goals_of(db, account).await? else {
        return Ok(None);
    };
    if profile.is_empty() || answered(&goals).0 == 0 {
        return Ok(None);
    }
    Ok(Some(Matching {
        profile,
        profile_updated_at,
        goals,
        goals_version,
    }))
}

/// The accounts with a profile and at least one goal answered: the ones a
/// nightly search runs for.
pub async fn accounts_with_goals(db: &PgPool) -> Result<Vec<Uuid>> {
    let accounts: Vec<(Uuid, sqlx::types::Json<Goals>)> = sqlx::query_as(
        "SELECT g.account_id, g.answers FROM profile.goals g
           JOIN profile.profiles p ON p.account_id = g.account_id
          ORDER BY g.account_id",
    )
    .fetch_all(db)
    .await?;
    Ok(accounts
        .into_iter()
        .filter(|(_, goals)| answered(&goals.0).0 > 0)
        .map(|(account, _)| account)
        .collect())
}

fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

/// The year of a `YYYY…` date.
fn year_of(date: &str) -> Option<i32> {
    date.get(..4)?.parse().ok()
}

/// A compact summary of `profile` for a model to match jobs against: the
/// headline, where they are, positions (title, company, years), skills,
/// languages, certifications, schools and roughly how many years they have
/// worked. Never their name, email or links.
pub fn summary(profile: &Profile, now: DateTime<Utc>) -> Value {
    let first_year = profile
        .work
        .iter()
        .filter_map(|w| w.start_date.as_deref().and_then(year_of))
        .min();
    let positions: Vec<Value> = profile
        .work
        .iter()
        .take(12)
        .map(|w| {
            json!({
                "title": w.position,
                "company": w.name,
                "from": w.start_date,
                "to": w.end_date,
                "summary": w.summary.as_deref().map(|s| cut(s, 300)),
            })
        })
        .collect();
    json!({
        "headline": profile.basics.label,
        "location": profile.basics.location,
        "about": profile.basics.summary.as_deref().map(|s| cut(s, 600)),
        "years_working": first_year.map(|year| (now.year() - year).max(0)),
        "positions": positions,
        "skills": profile.skills.iter().take(40).map(|s| s.name.as_str()).collect::<Vec<_>>(),
        "languages": profile
            .languages
            .iter()
            .map(|l| match &l.fluency {
                Some(fluency) => format!("{} ({fluency})", l.language),
                None => l.language.clone(),
            })
            .collect::<Vec<_>>(),
        "certifications": profile.certificates.iter().take(10).map(|c| c.name.as_str()).collect::<Vec<_>>(),
        "education": profile
            .education
            .iter()
            .take(5)
            .map(|e| json!({ "school": e.institution, "degree": e.study_type, "field": e.area }))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use crate::{
        goals::{CashFloor, Period, WorkMode},
        schema::{Basics, Skill, Work},
    };

    use super::*;

    #[test]
    fn answered_questions_are_counted_and_equity_only_with_a_floor() {
        assert_eq!(answered(&Goals::default()), (0, 11));
        let goals = Goals {
            roles: vec!["Designer".into()],
            work_mode: vec![WorkMode::Remote],
            cash_floor: Some(CashFloor {
                amount: 1,
                currency: "EUR".into(),
                period: Period::Year,
            }),
            equity_counts: Some(true),
            ..Goals::default()
        };
        assert_eq!(answered(&goals), (4, 12));
    }

    #[test]
    fn the_summary_leaves_out_who_they_are() {
        let profile = Profile {
            basics: Basics {
                name: Some("Alex Demo".into()),
                email: Some("alex@example.com".into()),
                label: Some("Engineer".into()),
                ..Basics::default()
            },
            work: vec![Work {
                name: Some("Example Org".into()),
                position: Some("Senior Engineer".into()),
                start_date: Some("2018-03".into()),
                ..Work::default()
            }],
            skills: vec![Skill {
                name: "Rust".into(),
                level: None,
            }],
            ..Profile::default()
        };
        let now = Utc.with_ymd_and_hms(2026, 10, 26, 0, 0, 0).unwrap();
        let summary = summary(&profile, now).to_string();
        assert!(!summary.contains("Alex"), "{summary}");
        assert!(!summary.contains("alex@example.com"), "{summary}");
        assert!(summary.contains("Senior Engineer"), "{summary}");
        assert!(summary.contains("\"years_working\":8"), "{summary}");
        assert_eq!(strength_hint(&profile), Some(StrengthHint::AddSkills));
    }
}
