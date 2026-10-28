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

use crate::{
    features::completion,
    goals::Goals,
    schema::Profile,
    strength::{self, Strength},
};

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

/// Where the person should go to make a weak profile stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileAction {
    /// The Improve flow: questions about the gaps.
    Improve,
    /// Skills found in their roles.
    Skills,
    /// Import a fuller profile.
    Import,
    /// Answer the goals.
    Goals,
}

impl ProfileAction {
    /// Lower-case name, as the API says it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Improve => "improve",
            Self::Skills => "skills",
            Self::Import => "import",
            Self::Goals => "goals",
        }
    }
}

/// How far a person is with their profile and goals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// How strong the profile is, 0 to 100 ([`crate::strength`]).
    pub strength: u8,
    /// Where to go to make it stronger; none once it is strong.
    pub next_action: Option<ProfileAction>,
    /// Open questions of the current gap review; none when there is no
    /// review for the current profile (yet).
    pub questions_left: Option<u32>,
    /// They have a profile with something in it.
    pub profile_done: bool,
    /// What would make it stronger.
    pub strength_hint: Option<StrengthHint>,
    /// It lists no skills and its text names no technology ([`no_stack`]).
    pub no_skills: bool,
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

/// Every string a JSON value holds (never its keys).
fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// Whether `profile` says nothing about a stack: it lists no skills, and
/// nothing a search sends to the model ([`summary`]) names a technology.
/// Matches cannot cite a stack the profile does not have.
pub fn no_stack(profile: &Profile) -> bool {
    if !profile.skills.is_empty() {
        return false;
    }
    let mut texts = Vec::new();
    strings(&summary(profile, DateTime::UNIX_EPOCH), &mut texts);
    !texts
        .iter()
        .any(|text| applicamp_kernel::tech::mentions(text))
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

/// Where a weak profile should go next: the questions when there are any,
/// else the heaviest part that is missing.
fn next_action(strength: &Strength, questions_left: Option<u32>) -> Option<ProfileAction> {
    if strength.is_strong() {
        return None;
    }
    if questions_left.is_some_and(|n| n > 0) {
        return Some(ProfileAction::Improve);
    }
    let heaviest = strength
        .items
        .iter()
        .filter(|item| item.state != strength::StrengthState::Ok)
        .max_by_key(|item| item.key.weight() - item.earned)?;
    Some(match heaviest.key {
        strength::StrengthKey::Skills => ProfileAction::Skills,
        strength::StrengthKey::Positions => ProfileAction::Import,
        strength::StrengthKey::Goals => ProfileAction::Goals,
        _ => ProfileAction::Improve,
    })
}

/// How far `account` is.
pub async fn status(db: &PgPool, account: Uuid) -> Result<Status> {
    let current = completion::current(db, account).await?;
    let profile = current
        .as_ref()
        .map(|c| &c.profile)
        .filter(|p| !p.is_empty());
    let goals = current.as_ref().map(|c| &c.goals);
    let (goals_answered, goals_total) = goals.map_or((0, GOAL_QUESTIONS), answered);
    let strength = strength::compute(
        profile.unwrap_or(&Profile::default()),
        goals.unwrap_or(&Goals::default()),
    );
    let questions_left = match (&current, profile) {
        (Some(current), Some(_)) => completion::questions_left(db, account, current).await?,
        _ => None,
    };
    Ok(Status {
        strength: strength.score,
        next_action: profile.and_then(|_| next_action(&strength, questions_left)),
        questions_left,
        profile_done: profile.is_some(),
        strength_hint: profile.and_then(strength_hint),
        no_skills: profile.is_some_and(no_stack),
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

/// Language names as people write them on a profile (English, the app's
/// languages and the language's own name), folded to lower case without
/// accents, and their ISO 639-1 codes.
const LANGUAGE_NAMES: &[(&str, &[&str])] = &[
    ("en", &["english", "ingles", "anglais", "englisch"]),
    (
        "pt",
        &["portuguese", "portugues", "portugais", "portugiesisch"],
    ),
    (
        "es",
        &[
            "spanish",
            "espanol",
            "castellano",
            "espanhol",
            "espagnol",
            "spanisch",
        ],
    ),
    ("fr", &["french", "francais", "frances", "franzosisch"]),
    ("de", &["german", "deutsch", "alemao", "aleman", "allemand"]),
    ("it", &["italian", "italiano", "italien", "italienisch"]),
    (
        "nl",
        &[
            "dutch",
            "nederlands",
            "holandes",
            "neerlandais",
            "niederlandisch",
            "flemish",
        ],
    ),
    (
        "pl",
        &["polish", "polski", "polaco", "polonais", "polnisch"],
    ),
    (
        "sv",
        &["swedish", "svenska", "sueco", "suedois", "schwedisch"],
    ),
    (
        "da",
        &["danish", "dansk", "dinamarques", "danois", "danisch"],
    ),
    (
        "no",
        &["norwegian", "norsk", "noruegues", "norvegien", "norwegisch"],
    ),
    (
        "fi",
        &["finnish", "suomi", "finlandes", "finnois", "finnisch"],
    ),
    (
        "cs",
        &["czech", "cestina", "checo", "tcheque", "tschechisch"],
    ),
    (
        "ro",
        &[
            "romanian",
            "romana",
            "romeno",
            "rumano",
            "roumain",
            "rumanisch",
        ],
    ),
    (
        "hu",
        &["hungarian", "magyar", "hungaro", "hongrois", "ungarisch"],
    ),
    ("el", &["greek", "grego", "griego", "grec", "griechisch"]),
    ("ru", &["russian", "russo", "ruso", "russe", "russisch"]),
    ("uk", &["ukrainian", "ucraniano", "ukrainien", "ukrainisch"]),
    ("tr", &["turkish", "turco", "turc", "turkisch"]),
    ("ar", &["arabic", "arabe", "arabisch"]),
    (
        "he",
        &["hebrew", "hebraico", "hebreo", "hebreu", "hebraisch"],
    ),
    ("hi", &["hindi"]),
    (
        "zh",
        &[
            "chinese",
            "mandarin",
            "chines",
            "chino",
            "chinois",
            "chinesisch",
        ],
    ),
    ("ja", &["japanese", "japones", "japonais", "japanisch"]),
    ("ko", &["korean", "coreano", "coreen", "koreanisch"]),
    ("ca", &["catalan", "catala"]),
    ("gl", &["galician", "galego", "gallego"]),
    ("eu", &["basque", "euskara", "basco", "vasco"]),
];

/// `text` in lower case without accents.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'č' => 'c',
            'š' => 's',
            other => other,
        })
        .collect()
}

/// The ISO 639-1 code of a language as a profile names it ("Portuguese",
/// "Português (Brasil)", "pt"); `None` for one the table does not know.
pub fn language_code(name: &str) -> Option<&'static str> {
    let folded = fold(name);
    let first = folded
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())?;
    LANGUAGE_NAMES
        .iter()
        .find(|(code, names)| *code == first || names.contains(&first))
        .map(|(code, _)| *code)
}

/// Whether a profile's `fluency` is professional or better.
///
/// LinkedIn's levels: "Native or bilingual", "Full professional" and
/// "Professional working" are; "Limited working" and "Elementary" are not.
/// CEFR: B2, C1 and C2 are; A1 to B1 are not. Words: native, bilingual,
/// mother tongue, fluent, proficient, advanced, professional are;
/// limited, elementary, basic, beginner and a plain "intermediate" are not.
/// A language listed without a fluency counts: they named it.
pub fn professional(fluency: Option<&str>) -> bool {
    let Some(fluency) = fluency.map(fold) else {
        return true;
    };
    let words: Vec<&str> = fluency
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |word: &str| words.contains(&word);
    if [
        "limited",
        "elementary",
        "basic",
        "beginner",
        "a1",
        "a2",
        "b1",
    ]
    .iter()
    .any(|w| has(w))
    {
        return false;
    }
    if [
        "native",
        "bilingual",
        "mother",
        "fluent",
        "proficient",
        "proficiency",
        "advanced",
        "professional",
        "c1",
        "c2",
        "b2",
        "nativo",
        "nativa",
        "fluente",
        "fluide",
        "courant",
        "langue",
        "materna",
        "avancado",
        "avanzado",
        "avance",
    ]
    .iter()
    .any(|w| has(w))
    {
        return true;
    }
    // "Intermediate", "Conversational" and anything else.
    words.is_empty()
}

/// The languages a person works in, as tags: `goals.languages_for_work`,
/// else the profile's languages spoken at a professional level or better
/// ([`professional`]), as ISO 639-1 codes ([`language_code`]). Empty when
/// neither says: no language requirement.
pub fn work_languages(goals: &Goals, profile: &Profile) -> Vec<String> {
    if !goals.languages_for_work.is_empty() {
        return goals.languages_for_work.clone();
    }
    let mut codes: Vec<String> = Vec::new();
    for language in &profile.languages {
        if !professional(language.fluency.as_deref()) {
            continue;
        }
        if let Some(code) = language_code(&language.language)
            && !codes.iter().any(|c| c == code)
        {
            codes.push(code.to_owned());
        }
    }
    codes
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

    fn speaks(language: &str, fluency: Option<&str>) -> crate::schema::Language {
        crate::schema::Language {
            language: language.into(),
            fluency: fluency.map(Into::into),
        }
    }

    #[test]
    fn work_languages_come_from_the_goals_else_professional_profile_languages() {
        let profile = Profile {
            languages: vec![
                speaks("Português", Some("Native or bilingual proficiency")),
                speaks("English", Some("Full professional proficiency")),
                speaks("Spanish", Some("Professional working proficiency")),
                speaks("German", Some("Elementary proficiency")),
                speaks("French", Some("Limited working proficiency")),
                speaks("Italian", Some("B1")),
                speaks("Dutch", Some("C1")),
                speaks("Klingon", None),
                speaks("Catalan", None),
            ],
            ..Profile::default()
        };
        assert_eq!(
            work_languages(&Goals::default(), &profile),
            ["pt", "en", "es", "nl", "ca"]
        );
        let goals = Goals {
            languages_for_work: vec!["en".into(), "pt-PT".into()],
            ..Goals::default()
        };
        assert_eq!(work_languages(&goals, &profile), ["en", "pt-PT"]);
        assert!(work_languages(&Goals::default(), &Profile::default()).is_empty());
        assert!(!professional(Some("Intermediate")));
        assert!(professional(Some("Fluent")));
        assert!(professional(Some("Langue maternelle")));
        assert_eq!(language_code("Português (Brasil)"), Some("pt"));
        assert_eq!(language_code("de"), Some("de"));
    }

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

    #[test]
    fn a_profile_with_no_skills_and_no_stack_words_has_no_stack() {
        let bare = Profile {
            basics: Basics {
                label: Some("Product and operations lead".into()),
                ..Basics::default()
            },
            work: vec![Work {
                name: Some("Example Org".into()),
                position: Some("Head of Operations".into()),
                summary: Some("Ran the roadmap and led a team of five.".into()),
                ..Work::default()
            }],
            ..Profile::default()
        };
        assert!(no_stack(&bare));
        // A skill, or a stack word in a position, is a stack.
        let mut skilled = bare.clone();
        skilled.skills.push(Skill {
            name: "Ruby".into(),
            level: None,
        });
        assert!(!no_stack(&skilled));
        let mut said = bare;
        said.work[0].summary = Some("Built the billing platform in Rails.".into());
        assert!(!no_stack(&said));
    }
}
