//! What a model-structured draft gets wrong in a known way, put right in
//! code, with no model (#72).
//!
//! A LinkedIn PDF puts each position's place on the line under its dates
//! ("Lisbon, Portugal"), and a model can answer it as the position's
//! `summary`: a CV drafter then sees thirteen places and no work. So:
//!
//! - a `summary` that is a place (the position's own `location`, or a short
//!   line that reads like "City, Region, Country" and says no verb) moves to
//!   `location` (or goes, when the position already has one);
//! - a `summary` that only says the title or the company again goes;
//! - bullet lines in a `summary` become `highlights`;
//! - dates written as words or numbers (`Jul 2023`, `07/2023`, `2023-7`)
//!   become `YYYY-MM`, and an end date that says "Present" goes.
//!
//! Nothing is invented: a value only moves or goes, and the source check
//! still runs after this.

use applicamp_kernel::claims::squash;

use crate::schema::{Profile, Work};

use super::source_check::MONTHS;

/// Longest value that may be read as a place, in characters.
const MAX_PLACE: usize = 40;

/// Small words a place's name may hold in lower case ("Bruxelas e Região",
/// "Área Metropolitana de Lisboa", "Frankfurt am Main").
const CONNECTORS: [&str; 25] = [
    "e", "and", "et", "y", "i", "de", "da", "do", "das", "dos", "del", "la", "le", "les", "los",
    "las", "di", "du", "des", "of", "the", "am", "an", "der", "upon",
];

/// Words that say what someone did, or how they were employed: a line that
/// holds one is not a place. Lower case, in the app's four languages.
const NOT_A_PLACE: [&str; 72] = [
    "led",
    "lead",
    "leading",
    "built",
    "build",
    "building",
    "managed",
    "manage",
    "managing",
    "developed",
    "develop",
    "developing",
    "designed",
    "design",
    "designing",
    "created",
    "creating",
    "worked",
    "working",
    "ran",
    "running",
    "founded",
    "helped",
    "helping",
    "responsible",
    "owned",
    "shipped",
    "wrote",
    "writing",
    "taught",
    "teaching",
    "sold",
    "selling",
    "grew",
    "launched",
    "maintained",
    "implemented",
    "delivered",
    "supported",
    "coordinated",
    "consulting",
    "freelance",
    "self-employed",
    "contract",
    "internship",
    "full-time",
    "part-time",
    "present",
    "liderei",
    "liderou",
    "desenvolvi",
    "desenvolveu",
    "geri",
    "criei",
    "trabalhei",
    "responsável",
    "estágio",
    "presente",
    "atual",
    "lideré",
    "desarrollé",
    "gestioné",
    "trabajé",
    "responsable",
    "prácticas",
    "dirigé",
    "développé",
    "géré",
    "créé",
    "travaillé",
    "stage",
    "aujourd'hui",
];

/// What an end date says when the position has not ended.
const NOW: [&str; 14] = [
    "present",
    "now",
    "current",
    "today",
    "presente",
    "atual",
    "actual",
    "atualmente",
    "actualmente",
    "actualidad",
    "hoje",
    "hoy",
    "aujourd'hui",
    "présent",
];

/// Marks that open a bullet line.
const BULLETS: [char; 9] = ['•', '·', '-', '–', '—', '*', '▪', '●', '◦'];

/// Whether `value` is a place rather than a description: it is the
/// position's own `location`, or a short line that reads like "City,
/// Region, Country" (every word capitalised or a small joining word, no
/// digit, no sentence, no word that says what someone did).
pub(crate) fn is_place(value: &str, location: Option<&str>) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if let Some(location) = location.filter(|l| !l.trim().is_empty()) {
        // The same place, or one part of it ("Lisbon" of "Lisbon, Portugal").
        let parts = |s: &str| -> Vec<String> {
            s.split(',').map(squash).filter(|p| !p.is_empty()).collect()
        };
        let (said, place) = (squash(value), squash(location));
        if said == place || parts(location).contains(&said) || parts(value).contains(&place) {
            return true;
        }
    }
    if value.chars().count() > MAX_PLACE
        || value.ends_with('.')
        || value
            .chars()
            .any(|c| c.is_ascii_digit() || "!?;:()@/\"\n|".contains(c))
    {
        return false;
    }
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    parts.len() <= 4
        && parts.iter().all(|part| {
            let words: Vec<&str> = part.split_whitespace().collect();
            (1..=5).contains(&words.len())
                && words.iter().all(|word| {
                    let lower = word.to_lowercase();
                    !NOT_A_PLACE.contains(&lower.as_str())
                        && (word.chars().next().is_some_and(char::is_uppercase)
                            || CONNECTORS.contains(&lower.as_str()))
                })
                && words
                    .first()
                    .and_then(|w| w.chars().next())
                    .is_some_and(char::is_uppercase)
        })
}

/// Letters and digits only, in lower case: "Staff Engineer · Example Org"
/// and "staff engineer, example org" are the same.
fn letters(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether `summary` only says the position's title or company again.
pub(crate) fn repeats_title(summary: &str, work: &Work) -> bool {
    let said = letters(summary);
    if said.is_empty() {
        return false;
    }
    let title = work.position.as_deref().map(letters).unwrap_or_default();
    let company = work.name.as_deref().map(letters).unwrap_or_default();
    [
        title.clone(),
        company.clone(),
        format!("{title}{company}"),
        format!("{company}{title}"),
        format!("{title}at{company}"),
        format!("{title}em{company}"),
        format!("{title}en{company}"),
        format!("{title}chez{company}"),
    ]
    .iter()
    .any(|form| !form.is_empty() && *form == said)
}

/// The month `name` names (1 to 12), in any of the app's languages.
fn month_of(name: &str) -> Option<usize> {
    let name = name.trim_end_matches('.').to_lowercase();
    MONTHS
        .iter()
        .position(|names| names.contains(&name.as_str()))
        .map(|at| at + 1)
}

fn year_of(raw: &str) -> Option<u32> {
    (raw.len() == 4 && raw.chars().all(|c| c.is_ascii_digit()))
        .then(|| raw.parse().ok())
        .flatten()
        .filter(|year| (1900..=2100).contains(year))
}

fn month_number(raw: &str) -> Option<usize> {
    (1..=2)
        .contains(&raw.len())
        .then(|| raw.parse::<usize>().ok())
        .flatten()
        .filter(|m| (1..=12).contains(m))
}

/// `value` as `YYYY`, `YYYY-MM` or `YYYY-MM-DD` when it is a date written
/// another common way (`Jul 2023`, `julho de 2023`, `07/2023`, `2023-7`);
/// as it is otherwise (the limits then drop and count it).
pub(crate) fn normalised_date(value: &str) -> String {
    let trimmed = value.trim();
    let month = |year: u32, month: usize| format!("{year:04}-{month:02}");
    // 2023-07, 2023-7, 2023/07, 2023.07
    let numeric: Vec<&str> = trimmed.split(['-', '/', '.']).collect();
    if let [first, second] = numeric.as_slice() {
        if let (Some(year), Some(m)) = (year_of(first), month_number(second)) {
            return month(year, m);
        }
        if let (Some(m), Some(year)) = (month_number(first), year_of(second)) {
            return month(year, m);
        }
    }
    // Jul 2023, July 2023, julho de 2023, juil. 2023, Jul, 2023
    let words: Vec<String> = trimmed
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty() && !matches!(w.to_lowercase().as_str(), "de" | "of"))
        .map(str::to_owned)
        .collect();
    if let [name, year] = words.as_slice()
        && let (Some(m), Some(year)) = (month_of(name), year_of(year))
    {
        return month(year, m);
    }
    trimmed.to_owned()
}

/// Whether an end date says the position has not ended.
fn says_now(value: &str) -> bool {
    NOW.contains(&value.trim().trim_end_matches('.').to_lowercase().as_str())
}

/// A date put right; none when it says "Present".
fn date(value: Option<String>) -> Option<String> {
    value.filter(|v| !says_now(v)).map(|v| normalised_date(&v))
}

/// Lines of `summary` that open with a bullet, as highlights, and what is
/// left as the summary.
fn split_bullets(summary: &str) -> (Option<String>, Vec<String>) {
    let mut kept = Vec::new();
    let mut bullets = Vec::new();
    for line in summary.lines() {
        let trimmed = line.trim();
        match trimmed.strip_prefix(BULLETS) {
            Some(rest) if !rest.trim().is_empty() && rest.starts_with(char::is_whitespace) => {
                bullets.push(rest.trim().to_owned());
            }
            _ => kept.push(line),
        }
    }
    let rest = kept.join("\n").trim().to_owned();
    ((!rest.is_empty()).then_some(rest), bullets)
}

/// What [`repaired`] changed, counted (for the logs: never the values).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Repairs {
    /// Summaries that were a place.
    pub(crate) places: u32,
    /// Summaries that only said the title or the company again.
    pub(crate) titles: u32,
    /// Bullet lines taken out of a summary as highlights.
    pub(crate) bullets: u32,
}

/// One position put right.
fn repair_work(work: &mut Work, repairs: &mut Repairs) {
    work.start_date = date(work.start_date.take());
    work.end_date = date(work.end_date.take());
    if let Some(summary) = work.summary.take() {
        // The title said again first: "Staff Engineer" would read as a place.
        if repeats_title(&summary, work) {
            repairs.titles += 1;
        } else if is_place(&summary, work.location.as_deref()) {
            repairs.places += 1;
            if work.location.as_deref().is_none_or(|l| l.trim().is_empty()) {
                work.location = Some(summary.trim().to_owned());
            }
        } else {
            let (rest, bullets) = split_bullets(&summary);
            repairs.bullets += u32::try_from(bullets.len()).unwrap_or(u32::MAX);
            for bullet in bullets {
                add_highlight(&mut work.highlights, bullet);
            }
            work.summary = rest;
        }
    }
    let mut seen = Vec::new();
    work.highlights.retain(|h| {
        let key = squash(h);
        let new = !key.is_empty() && !seen.contains(&key);
        seen.push(key);
        new
    });
}

/// `highlight` added to `highlights` unless one says the same.
pub(crate) fn add_highlight(highlights: &mut Vec<String>, highlight: String) {
    let key = squash(&highlight);
    if !key.is_empty() && !highlights.iter().any(|h| squash(h) == key) {
        highlights.push(highlight);
    }
}

/// `draft` put right, and what was changed.
pub(crate) fn repaired(mut draft: Profile) -> (Profile, Repairs) {
    let mut repairs = Repairs::default();
    for work in &mut draft.work {
        repair_work(work, &mut repairs);
    }
    for school in &mut draft.education {
        school.start_date = date(school.start_date.take());
        school.end_date = date(school.end_date.take());
    }
    for project in &mut draft.projects {
        project.start_date = date(project.start_date.take());
        project.end_date = date(project.end_date.take());
    }
    for certificate in &mut draft.certificates {
        certificate.date = date(certificate.date.take());
    }
    (draft, repairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Education, Profile};

    fn position(summary: &str, location: Option<&str>) -> Work {
        Work {
            name: Some("Example Org".into()),
            position: Some("Staff Engineer".into()),
            location: location.map(str::to_owned),
            start_date: Some("Jul 2023".into()),
            end_date: Some("Apr 2024".into()),
            summary: Some(summary.into()),
            ..Work::default()
        }
    }

    #[test]
    fn places_are_places() {
        for place in [
            "Lisboa, Lisboa, Portugal",
            "Bruxelas e Região",
            "Lisbon",
            "Greater Lisbon Area",
            "Área Metropolitana de Lisboa",
            "Porto, Portugal",
            "Frankfurt am Main, Germany",
            "Région de Bruxelles-Capitale, Belgique",
            "St. Gallen, Switzerland",
        ] {
            assert!(is_place(place, None), "{place}");
        }
        for not in [
            "Built the billing platform.",
            "Led the platform team",
            "Led Engineering",
            "Full-time",
            "Freelance",
            "Jul 2023 - Apr 2024 (10 months)",
            "I build boring, reliable backends for small teams.",
            "Responsible for payments, invoicing and the team",
            "Runs the billing platform in Rust, with Kafka between its services, for 40 shops",
            "",
        ] {
            assert!(!is_place(not, None), "{not}");
        }
        // The position's own location is a place, however it is written.
        assert!(is_place("lisbon,  portugal", Some("Lisbon, Portugal")));
        assert!(is_place("Lisbon", Some("Lisbon, Portugal")));
        assert!(!is_place(
            "Built the billing platform for shops in Lisbon, Portugal and Spain.",
            Some("Lisbon, Portugal")
        ));
    }

    #[test]
    fn a_place_in_the_summary_moves_to_the_location() {
        let (draft, repairs) = repaired(Profile {
            work: vec![
                position("Lisboa, Lisboa, Portugal", None),
                position("Lisbon", Some("Lisbon, Portugal")),
                position("Built the billing platform.", Some("Porto, Portugal")),
            ],
            ..Profile::default()
        });
        assert_eq!(draft.work[0].summary, None);
        assert_eq!(
            draft.work[0].location.as_deref(),
            Some("Lisboa, Lisboa, Portugal")
        );
        assert_eq!(draft.work[1].summary, None, "never a place in a summary");
        assert_eq!(draft.work[1].location.as_deref(), Some("Lisbon, Portugal"));
        assert_eq!(
            draft.work[2].summary.as_deref(),
            Some("Built the billing platform.")
        );
        assert_eq!(repairs.places, 2);
    }

    #[test]
    fn a_summary_that_repeats_the_title_goes() {
        for said in [
            "Staff Engineer",
            "Example Org",
            "Staff Engineer · Example Org",
            "Staff Engineer at Example Org",
        ] {
            let (draft, repairs) = repaired(Profile {
                work: vec![position(said, Some("Lisbon"))],
                ..Profile::default()
            });
            assert_eq!(draft.work[0].summary, None, "{said}");
            assert_eq!(repairs.titles, 1);
        }
    }

    #[test]
    fn bullet_lines_become_highlights() {
        let mut work = position(
            "Ran the payments team.\n• Cut refunds by half\n- Moved billing to Rust\n• Cut refunds by half",
            None,
        );
        work.highlights = vec!["Moved billing to Rust".into()];
        let (draft, repairs) = repaired(Profile {
            work: vec![work],
            ..Profile::default()
        });
        assert_eq!(
            draft.work[0].summary.as_deref(),
            Some("Ran the payments team.")
        );
        assert_eq!(
            draft.work[0].highlights,
            ["Moved billing to Rust", "Cut refunds by half"]
        );
        assert_eq!(repairs.bullets, 3);
    }

    #[test]
    fn dates_become_years_and_months() {
        for (written, want) in [
            ("Jul 2023", "2023-07"),
            ("July 2023", "2023-07"),
            ("julho de 2023", "2023-07"),
            ("juil. 2023", "2023-07"),
            ("Sept 2019", "2019-09"),
            ("07/2023", "2023-07"),
            ("7/2023", "2023-07"),
            ("2023-7", "2023-07"),
            ("2023/07", "2023-07"),
            ("2023", "2023"),
            ("2023-07", "2023-07"),
            ("2023-07-15", "2023-07-15"),
            ("sometime", "sometime"),
        ] {
            assert_eq!(normalised_date(written), want, "{written}");
        }
        let (draft, _) = repaired(Profile {
            work: vec![Work {
                end_date: Some("Present".into()),
                ..position("Built it.", None)
            }],
            education: vec![Education {
                institution: "Sample University".into(),
                start_date: Some("Sep 2012".into()),
                end_date: Some("2017".into()),
                ..Education::default()
            }],
            ..Profile::default()
        });
        assert_eq!(draft.work[0].start_date.as_deref(), Some("2023-07"));
        assert_eq!(draft.work[0].end_date, None, "Present is no end date");
        assert_eq!(draft.education[0].start_date.as_deref(), Some("2012-09"));
    }
}
