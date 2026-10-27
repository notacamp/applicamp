//! Hard requirements are pass/fail, decided here, never by the score.
//!
//! The model reads **facts** from each posting in the scoring call
//! ([`Facts`]: remote, open to where the person lives, the languages it
//! needs, the pay it states); these rules decide from the facts and the
//! goals. A job that fails one is never added, whatever its score.
//!
//! | Goal | Passes when |
//! |---|---|
//! | `work_mode` = only remote | `remote = yes` and `open_to_candidate = yes` |
//! | `work_mode` includes hybrid or on site | `remote = yes` and `open_to_candidate = yes`, or the job's place is one of `locations` (or within `relocate`'s reach) |
//! | `relocate` = no | `open_to_candidate = yes`, or the job's place is one of `locations` |
//! | languages to work in | every language the job needs is one of them |
//! | `cash_floor` | the stated pay's top, converted, is at least the floor; none stated passes with the gap "Salary not stated" |
//!
//! - **Unclear is not met**: a fact the posting does not state fails the
//!   requirement it is for. A requirement the person did not set is not
//!   checked. When where they live is unknown, `open_to_candidate` is not
//!   asked of a remote job (nobody could answer it).
//! - **Where they live** is the first of `goals.locations`, else the
//!   profile's location; **their places** are `goals.locations`, else that
//!   one ([`crate::places`] says whether a job's place is one of them).
//! - **Their languages** are `goals.languages_for_work`, else the profile's
//!   languages spoken at a professional level or better (the server maps
//!   them); English counts only when listed.
//! - **Pay** is converted with a fixed table ([`EUR_PER_UNIT`]: EUR, USD,
//!   GBP, CHF) and twelve months a year. Pay in another currency than the
//!   floor's and outside the table, or per week, day or hour, is not
//!   compared: it passes with a gap saying so.
//! - `avoid` and everything else stay in the model's score.

use serde::{Deserialize, Serialize};

use crate::{job::WorkMode, matching::MatchInput, places};

/// The rules' version. Scores are cached per version: a change to what the
/// model is asked re-asks it, once per job.
pub const RULES_VERSION: i32 = 1;

/// The gap a job without a stated salary gets, when there is a floor.
pub const SALARY_NOT_STATED: &str = "Salary not stated";

/// Euros per unit of each currency the floor is compared in. Fixed, so a
/// decision never moves with a rate feed; close to the 2026 averages.
pub const EUR_PER_UNIT: [(&str, f64); 4] =
    [("EUR", 1.0), ("USD", 0.92), ("GBP", 1.17), ("CHF", 1.05)];

/// A fact the posting states, or not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// It says so.
    Yes,
    /// It says otherwise.
    No,
    /// It does not say.
    #[default]
    Unclear,
}

impl Answer {
    /// Lower-case name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Unclear => "unclear",
        }
    }

    /// `yes` or `no`; anything else is unclear.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "yes" => Self::Yes,
            "no" => Self::No,
            _ => Self::Unclear,
        }
    }
}

/// The pay a posting states.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Salary {
    /// The least, in `currency`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// The most.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// ISO 4217 (`EUR`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// `year`, `month`, `week`, `day` or `hour`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
}

/// What the model read from a posting about the requirements, and only
/// from the posting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// It can be done fully remote.
    pub remote: Answer,
    /// Someone living where the person lives can do it without moving.
    pub open_to_candidate: Answer,
    /// Where it is open, as the posting says ("Germany or Austria only").
    pub region_note: Option<String>,
    /// The languages it needs, ISO 639-1 (`en`, `de`).
    pub working_languages: Vec<String>,
    /// The pay it states.
    pub salary: Option<Salary>,
}

/// Why a job was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Excluded {
    /// They work remote only, and it is not (or does not say so).
    NotRemote,
    /// It cannot be done from where they live, nor is it where they can be.
    NotOpenToLocation,
    /// It needs a language they did not list.
    Language,
    /// Its stated pay is under their floor.
    BelowFloor,
}

impl Excluded {
    /// Every reason.
    pub const ALL: [Self; 4] = [
        Self::NotRemote,
        Self::NotOpenToLocation,
        Self::Language,
        Self::BelowFloor,
    ];

    /// Lower-case name, as stored and as the API says it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRemote => "not_remote",
            Self::NotOpenToLocation => "not_open_to_location",
            Self::Language => "language",
            Self::BelowFloor => "below_floor",
        }
    }

    /// The reason named `raw`.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == raw)
    }
}

/// How far they would move for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Nowhere.
    No,
    /// Within their country.
    WithinCountry,
    /// Within the EU.
    WithinEu,
    /// Anywhere.
    Anywhere,
}

/// What a floor is counted over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Per {
    Year,
    Month,
}

/// The least pay they take, before tax.
#[derive(Debug, Clone, PartialEq)]
pub struct Floor {
    /// In `currency`, per `per`.
    pub amount: f64,
    /// ISO 4217, in capitals.
    pub currency: String,
    pub per: Per,
}

/// What the rules decided about a job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Why it is left out; `None` when it meets every requirement.
    pub excluded: Option<Excluded>,
    /// What the person should know about a job that passes ("Salary not
    /// stated").
    pub gaps: Vec<String>,
}

/// A language tag (`pt-PT`, `EN`, `nb`) as the ISO 639-1 code the rules
/// compare (`pt`, `en`, `no`); `None` when it is not a language tag.
pub fn language_code(tag: &str) -> Option<String> {
    let primary = tag.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(match primary.as_str() {
        "nb" | "nn" => "no".to_owned(),
        _ => primary,
    })
}

/// The places a person can work at: their `locations`, else where they
/// live.
fn places_of(input: &MatchInput) -> Vec<&str> {
    if input.locations.is_empty() {
        input.home.as_deref().into_iter().collect()
    } else {
        input.locations.iter().map(String::as_str).collect()
    }
}

/// The job, at `job_location`, is at one of their places.
fn at_their_places(places: &[&str], job_location: Option<&str>) -> bool {
    job_location.is_some_and(|job| places.iter().any(|place| places::same_place(job, place)))
}

/// The job, at `job_location`, is one of their places or within the reach
/// of how far they would move. With no place known and no reach to judge,
/// there is no place requirement.
fn in_reach(input: &MatchInput, job_location: Option<&str>) -> bool {
    if input.relocate == Some(Reach::Anywhere) {
        return true;
    }
    let places = places_of(input);
    if places.is_empty() {
        return true;
    }
    if at_their_places(&places, job_location) {
        return true;
    }
    let Some(job) = job_location else {
        return false;
    };
    let job_countries = places::countries_in(job);
    match input.relocate {
        Some(Reach::WithinCountry) => {
            let theirs: Vec<&str> = places
                .iter()
                .flat_map(|place| places::countries_in(place))
                .collect();
            job_countries.iter().any(|c| theirs.contains(c))
        }
        Some(Reach::WithinEu) => job_countries.iter().any(|c| places::is_eu(c)),
        _ => false,
    }
}

/// How they want to work, and where.
fn place_rule(input: &MatchInput, facts: &Facts, job_location: Option<&str>) -> Option<Excluded> {
    // Doable from where they live: remote, and open to it (when known).
    let from_home = facts.remote == Answer::Yes
        && (input.home.is_none() || facts.open_to_candidate == Answer::Yes);
    let office = input.work_modes.iter().any(|m| *m != WorkMode::Remote);
    if input.remote_only() {
        if facts.remote != Answer::Yes {
            return Some(Excluded::NotRemote);
        }
        if !from_home {
            return Some(Excluded::NotOpenToLocation);
        }
    } else if office && !from_home && !in_reach(input, job_location) {
        return Some(Excluded::NotOpenToLocation);
    }
    if input.relocate == Some(Reach::No) {
        let places = places_of(input);
        if !places.is_empty()
            && facts.open_to_candidate != Answer::Yes
            && !at_their_places(&places, job_location)
        {
            return Some(Excluded::NotOpenToLocation);
        }
    }
    None
}

/// The languages it needs, against theirs.
fn language_rule(input: &MatchInput, facts: &Facts) -> Option<Excluded> {
    if input.languages.is_empty() {
        return None;
    }
    facts
        .working_languages
        .iter()
        .filter_map(|tag| language_code(tag))
        .any(|code| !input.languages.contains(&code))
        .then_some(Excluded::Language)
}

/// `amount` in `currency` per `period`, in the floor's currency and period;
/// `None` when the table cannot convert it.
pub fn convert(amount: f64, currency: &str, period: &str, floor: &Floor) -> Option<f64> {
    let currency = currency.trim().to_ascii_uppercase();
    let rate = if currency == floor.currency {
        1.0
    } else {
        let eur = |code: &str| {
            EUR_PER_UNIT
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, rate)| *rate)
        };
        eur(&currency)? / eur(&floor.currency)?
    };
    let per = match period.trim().to_ascii_lowercase().as_str() {
        "year" | "yearly" | "annual" | "annually" => Per::Year,
        "month" | "monthly" => Per::Month,
        _ => return None,
    };
    let months = match (per, floor.per) {
        (Per::Year, Per::Month) => 1.0 / 12.0,
        (Per::Month, Per::Year) => 12.0,
        _ => 1.0,
    };
    Some(amount * rate * months)
}

/// The stated pay, against their floor.
fn floor_rule(input: &MatchInput, facts: &Facts, gaps: &mut Vec<String>) -> Option<Excluded> {
    let floor = input.cash_floor.as_ref()?;
    let stated = facts.salary.as_ref().and_then(|salary| {
        salary
            .max
            .or(salary.min)
            .filter(|amount| amount.is_finite() && *amount > 0.0)
            .map(|amount| (amount, salary))
    });
    let Some((top, salary)) = stated else {
        gaps.push(SALARY_NOT_STATED.to_owned());
        return None;
    };
    let currency = salary.currency.as_deref().unwrap_or("");
    let period = salary.period.as_deref().unwrap_or("");
    if let Some(amount) = convert(top, currency, period, floor) {
        return (amount < floor.amount).then_some(Excluded::BelowFloor);
    }
    let currency = if currency.is_empty() {
        "an unstated currency"
    } else {
        currency
    };
    let period = if period.is_empty() {
        "an unstated period"
    } else {
        period
    };
    gaps.push(format!(
        "Pay stated in {currency} per {period}, not compared with your floor"
    ));
    None
}

/// Whether a job meets `input`'s requirements, from the `facts` read from
/// its posting and where it is (`job_location`, as the board or the model
/// wrote it).
pub fn check(input: &MatchInput, facts: &Facts, job_location: Option<&str>) -> Verdict {
    let mut gaps = Vec::new();
    let excluded = place_rule(input, facts, job_location)
        .or_else(|| language_rule(input, facts))
        .or_else(|| floor_rule(input, facts, &mut gaps));
    if excluded.is_some() {
        gaps.clear();
    }
    Verdict { excluded, gaps }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Someone in Lisbon who works remote only, in English and Portuguese,
    /// and would not move.
    fn lisbon_remote_only() -> MatchInput {
        MatchInput {
            work_modes: vec![WorkMode::Remote],
            locations: vec!["Lisbon".into()],
            home: Some("Lisbon".into()),
            relocate: Some(Reach::No),
            will_not_relocate: true,
            languages: vec!["en".into(), "pt".into()],
            ..MatchInput::default()
        }
    }

    fn facts(remote: Answer, open: Answer, languages: &[&str]) -> Facts {
        Facts {
            remote,
            open_to_candidate: open,
            region_note: None,
            working_languages: languages.iter().map(|l| (*l).to_owned()).collect(),
            salary: None,
        }
    }

    fn excluded(input: &MatchInput, facts: &Facts, place: Option<&str>) -> Option<&'static str> {
        check(input, facts, place).excluded.map(Excluded::as_str)
    }

    use Answer::{No, Unclear, Yes};

    #[test]
    fn remote_only_needs_remote_and_open_to_where_they_live() {
        let input = lisbon_remote_only();
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["en"]), Some("Remote (EU)")),
            None
        );
        // "Remote-first from Germany or Austria".
        assert_eq!(
            excluded(&input, &facts(Yes, No, &["en"]), Some("Berlin")),
            Some("not_open_to_location")
        );
        assert_eq!(
            excluded(&input, &facts(Yes, Unclear, &["en"]), Some("Remote")),
            Some("not_open_to_location"),
            "unclear is not met"
        );
        // A Portugal role that never says remote.
        assert_eq!(
            excluded(&input, &facts(Unclear, Yes, &["en"]), Some("Portugal")),
            Some("not_remote")
        );
        assert_eq!(
            excluded(&input, &facts(No, Yes, &["en"]), Some("Lisbon")),
            Some("not_remote"),
            "an office in their own city is still not remote"
        );
        // Nowhere known to live: only "remote" can be asked.
        let nowhere = MatchInput {
            locations: vec![],
            home: None,
            ..lisbon_remote_only()
        };
        assert_eq!(
            excluded(&nowhere, &facts(Yes, Unclear, &["en"]), None),
            None
        );
        assert_eq!(
            excluded(&nowhere, &facts(Unclear, Unclear, &["en"]), None),
            Some("not_remote")
        );
    }

    #[test]
    fn hybrid_or_on_site_passes_remote_from_home_or_their_places() {
        let input = MatchInput {
            work_modes: vec![WorkMode::Remote, WorkMode::Hybrid],
            relocate: None,
            will_not_relocate: false,
            ..lisbon_remote_only()
        };
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["en"]), Some("Remote")),
            None
        );
        assert_eq!(
            excluded(&input, &facts(No, Yes, &["en"]), Some("Lisbon, Portugal")),
            None
        );
        assert_eq!(
            excluded(&input, &facts(No, No, &["en"]), Some("Berlin, Germany")),
            Some("not_open_to_location")
        );
        assert_eq!(
            excluded(&input, &facts(Yes, No, &["en"]), Some("Remote - US")),
            Some("not_open_to_location")
        );
        assert_eq!(
            excluded(&input, &facts(Unclear, Unclear, &["en"]), None),
            Some("not_open_to_location"),
            "no place to judge is not met"
        );
        // Within their reach when they would move.
        let within_country = MatchInput {
            relocate: Some(Reach::WithinCountry),
            ..input.clone()
        };
        assert_eq!(
            excluded(&within_country, &facts(No, No, &["en"]), Some("Porto")),
            None
        );
        assert_eq!(
            excluded(&within_country, &facts(No, No, &["en"]), Some("Madrid")),
            Some("not_open_to_location")
        );
        let within_eu = MatchInput {
            relocate: Some(Reach::WithinEu),
            ..input.clone()
        };
        assert_eq!(
            excluded(&within_eu, &facts(No, No, &["en"]), Some("Madrid, Spain")),
            None
        );
        assert_eq!(
            excluded(&within_eu, &facts(No, No, &["en"]), Some("Zurich")),
            Some("not_open_to_location")
        );
        let anywhere = MatchInput {
            relocate: Some(Reach::Anywhere),
            ..input
        };
        assert_eq!(
            excluded(&anywhere, &facts(No, No, &["en"]), Some("Toronto")),
            None
        );
    }

    #[test]
    fn not_relocating_needs_open_to_them_or_their_places() {
        let input = MatchInput {
            work_modes: vec![],
            ..lisbon_remote_only()
        };
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["en"]), Some("Remote")),
            None
        );
        assert_eq!(
            excluded(&input, &facts(No, Unclear, &["en"]), Some("Lisbon")),
            None
        );
        assert_eq!(
            excluded(&input, &facts(No, Unclear, &["en"]), Some("Berlin")),
            Some("not_open_to_location")
        );
        // No answer to "how": no work-mode rule; no answer to "move": none.
        let open = MatchInput {
            relocate: None,
            will_not_relocate: false,
            ..input
        };
        assert_eq!(
            excluded(&open, &facts(No, No, &["en"]), Some("Berlin")),
            None
        );
    }

    #[test]
    fn every_language_it_needs_must_be_theirs() {
        let input = lisbon_remote_only();
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["en", "pt-PT"]), None),
            None
        );
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["de"]), None),
            Some("language")
        );
        assert_eq!(
            excluded(&input, &facts(Yes, Yes, &["en", "de"]), None),
            Some("language")
        );
        // English only when listed.
        let portuguese = MatchInput {
            languages: vec!["pt".into()],
            ..lisbon_remote_only()
        };
        assert_eq!(
            excluded(&portuguese, &facts(Yes, Yes, &["en"]), None),
            Some("language")
        );
        // No languages known: no language requirement.
        let any = MatchInput {
            languages: vec![],
            ..lisbon_remote_only()
        };
        assert_eq!(excluded(&any, &facts(Yes, Yes, &["de"]), None), None);
        assert_eq!(language_code("pt-PT").as_deref(), Some("pt"));
        assert_eq!(language_code("NB").as_deref(), Some("no"));
        assert_eq!(language_code("German"), None);
    }

    fn paid(min: Option<f64>, max: Option<f64>, currency: &str, period: &str) -> Facts {
        Facts {
            salary: Some(Salary {
                min,
                max,
                currency: Some(currency.into()),
                period: Some(period.into()),
            }),
            ..facts(Yes, Yes, &["en"])
        }
    }

    #[test]
    fn the_stated_pay_meets_the_floor_converted_by_the_table() {
        let input = MatchInput {
            cash_floor: Some(Floor {
                amount: 60_000.0,
                currency: "EUR".into(),
                per: Per::Year,
            }),
            ..lisbon_remote_only()
        };
        let verdict = |f: &Facts| check(&input, f, Some("Remote"));
        assert_eq!(
            verdict(&paid(Some(50_000.0), Some(70_000.0), "EUR", "year")).excluded,
            None
        );
        assert_eq!(
            verdict(&paid(Some(40_000.0), Some(55_000.0), "EUR", "year")).excluded,
            Some(Excluded::BelowFloor)
        );
        // 5 500 a month is 66 000 a year.
        assert_eq!(
            verdict(&paid(None, Some(5_500.0), "EUR", "month")).excluded,
            None
        );
        // $60 000 is about €55 200.
        assert_eq!(
            verdict(&paid(None, Some(60_000.0), "usd", "year")).excluded,
            Some(Excluded::BelowFloor)
        );
        // £55 000 is about €64 350.
        assert_eq!(
            verdict(&paid(None, Some(55_000.0), "GBP", "year")).excluded,
            None
        );
        // Only a minimum stated: that is its top.
        assert_eq!(
            verdict(&paid(Some(45_000.0), None, "EUR", "year")).excluded,
            Some(Excluded::BelowFloor)
        );
        // Not stated, or not comparable: passes, with a gap.
        let none = verdict(&facts(Yes, Yes, &["en"]));
        assert_eq!(none.excluded, None);
        assert_eq!(none.gaps, [SALARY_NOT_STATED]);
        let yen = verdict(&paid(None, Some(9_000_000.0), "JPY", "year"));
        assert_eq!(yen.excluded, None);
        assert!(yen.gaps[0].contains("JPY"), "{:?}", yen.gaps);
        let hourly = verdict(&paid(None, Some(50.0), "EUR", "hour"));
        assert_eq!(hourly.excluded, None);
        assert_eq!(hourly.gaps.len(), 1);
        // A floor in a currency outside the table compares only like with like.
        let sek = MatchInput {
            cash_floor: Some(Floor {
                amount: 40_000.0,
                currency: "SEK".into(),
                per: Per::Month,
            }),
            ..lisbon_remote_only()
        };
        assert_eq!(
            check(&sek, &paid(None, Some(35_000.0), "SEK", "month"), None).excluded,
            Some(Excluded::BelowFloor)
        );
        assert_eq!(
            check(&sek, &paid(None, Some(90_000.0), "EUR", "year"), None).excluded,
            None
        );
    }

    #[test]
    fn the_first_rule_failed_is_the_reason_and_no_gaps_come_with_it() {
        let input = MatchInput {
            cash_floor: Some(Floor {
                amount: 1.0,
                currency: "EUR".into(),
                per: Per::Year,
            }),
            ..lisbon_remote_only()
        };
        let verdict = check(&input, &facts(Yes, No, &["de"]), Some("Berlin"));
        assert_eq!(verdict.excluded, Some(Excluded::NotOpenToLocation));
        assert!(verdict.gaps.is_empty());
        assert_eq!(Excluded::parse("language"), Some(Excluded::Language));
        assert_eq!(Answer::parse(" YES "), Answer::Yes);
        assert_eq!(Answer::parse("maybe"), Answer::Unclear);
    }
}
