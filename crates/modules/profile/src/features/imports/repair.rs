//! What a model-structured draft gets wrong in a known way, put right in
//! code, with no model (#72).
//!
//! A LinkedIn PDF puts each position's place on the line under its dates
//! ("Porto, Portugal"), and a model can answer it as the position's
//! `summary`: a CV drafter then sees a list of places and no work. So:
//!
//! - a `summary` that is a place moves to `location` (or goes, when the
//!   position already has one), and so does a place on the first line of a
//!   longer summary;
//! - a `summary` that only says the title or the company again goes;
//! - bullet lines in a `summary` become `highlights`;
//! - dates written as words or numbers (`Jul 2023`, `07/2023`, `15/03/2021`,
//!   `2023-7`) become `YYYY-MM` (or `YYYY-MM-DD`), and an end date that says
//!   "Present" goes.
//!
//! A place is told from a description without a model, and on the side of
//! caution: a line with a comma is a place when every part of it reads like a
//! name and it ends with a country, or it is the position's own location; a
//! line without one only when it is a part of the known location, a region
//! ("Greater Example Area", "Lisboa e região"), a country, or a place with a
//! way of working ("Porto - Remote"). "Head of Engineering" and "Rust, Kafka,
//! PostgreSQL" are never places.
//!
//! Nothing is invented: a value only moves or goes, and the source check
//! still runs after this.

use applicamp_kernel::{claims::squash, tech};

use crate::schema::{Profile, Work};

use super::source_check::MONTHS;

/// Longest line without a comma that may be read as a place, in characters.
const MAX_PLACE: usize = 40;
/// Longest "City, Region, Country" line, in characters ("Washington,
/// District of Columbia, United States").
const MAX_PLACE_PARTS: usize = 80;

/// Small words a place's name may hold in lower case ("Lisboa e região",
/// "Área Metropolitana de Lisboa", "Frankfurt am Main").
const CONNECTORS: [&str; 25] = [
    "e", "and", "et", "y", "i", "de", "da", "do", "das", "dos", "del", "la", "le", "les", "los",
    "las", "di", "du", "des", "of", "the", "am", "an", "der", "upon",
];

/// Words that make a line a region rather than a name ("Greater Example
/// Area", "Antuérpia e Região"). Lower case, in the app's four languages.
const REGION_WORDS: [&str; 18] = [
    "area",
    "área",
    "região",
    "regiao",
    "region",
    "région",
    "región",
    "metropolitan",
    "metropolitana",
    "metropolitaine",
    "greater",
    "grande",
    "grand",
    "county",
    "district",
    "distrito",
    "province",
    "província",
];

/// Ways of working a place line may end with ("Porto - Remote", "Porto,
/// Portugal (Remote)"), or be. Lower case.
const WORK_MODES: [&str; 17] = [
    "remote",
    "hybrid",
    "on-site",
    "onsite",
    "on site",
    "remoto",
    "remota",
    "híbrido",
    "híbrida",
    "hibrido",
    "presencial",
    "teletrabajo",
    "a distancia",
    "télétravail",
    "hybride",
    "sur site",
    "à distance",
];

/// Countries, as a place line ends ("…, Portugal"), in the app's four
/// languages. Lower case.
const COUNTRIES: [&str; 142] = [
    "portugal",
    "spain",
    "españa",
    "espanha",
    "espagne",
    "france",
    "frança",
    "francia",
    "germany",
    "deutschland",
    "alemanha",
    "alemania",
    "allemagne",
    "italy",
    "italia",
    "itália",
    "italie",
    "belgium",
    "bélgica",
    "belgique",
    "belgië",
    "netherlands",
    "the netherlands",
    "países baixos",
    "países bajos",
    "pays-bas",
    "nederland",
    "luxembourg",
    "luxemburgo",
    "switzerland",
    "suíça",
    "suiza",
    "suisse",
    "schweiz",
    "austria",
    "áustria",
    "autriche",
    "österreich",
    "ireland",
    "irlanda",
    "irlande",
    "united kingdom",
    "uk",
    "reino unido",
    "royaume-uni",
    "england",
    "scotland",
    "wales",
    "denmark",
    "dinamarca",
    "danemark",
    "sweden",
    "suécia",
    "suecia",
    "suède",
    "norway",
    "noruega",
    "norvège",
    "finland",
    "finlândia",
    "finlandia",
    "finlande",
    "iceland",
    "poland",
    "polónia",
    "polonia",
    "pologne",
    "czechia",
    "czech republic",
    "chéquia",
    "greece",
    "grécia",
    "grecia",
    "grèce",
    "romania",
    "roménia",
    "rumania",
    "roumanie",
    "hungary",
    "hungria",
    "hongrie",
    "croatia",
    "estonia",
    "latvia",
    "lithuania",
    "ukraine",
    "ucrânia",
    "turkey",
    "türkiye",
    "united states",
    "united states of america",
    "usa",
    "estados unidos",
    "états-unis",
    "canada",
    "canadá",
    "mexico",
    "méxico",
    "mexique",
    "brazil",
    "brasil",
    "brésil",
    "argentina",
    "argentine",
    "chile",
    "chili",
    "colombia",
    "colômbia",
    "colombie",
    "peru",
    "perú",
    "pérou",
    "uruguay",
    "uruguai",
    "angola",
    "mozambique",
    "moçambique",
    "cabo verde",
    "cape verde",
    "morocco",
    "marrocos",
    "marruecos",
    "maroc",
    "south africa",
    "nigeria",
    "egypt",
    "israel",
    "united arab emirates",
    "india",
    "índia",
    "inde",
    "china",
    "chine",
    "japan",
    "japão",
    "japón",
    "japon",
    "singapore",
    "australia",
    "austrália",
    "australie",
    "new zealand",
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
const NOW: [&str; 21] = [
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
    "la actualidad",
    "o momento",
    "momento",
    "até ao momento",
    "até o momento",
    "hoje",
    "hoy",
    "aujourd'hui",
    "à ce jour",
    "présent",
    "en cours",
];

/// Marks that open a bullet line.
const BULLETS: [char; 9] = ['•', '·', '-', '–', '—', '*', '▪', '●', '◦'];

/// `text` with typographic apostrophes made plain, in lower case.
fn plain(text: &str) -> String {
    text.trim().replace(['’', '‘'], "'").to_lowercase()
}

/// `value` without a way of working at its end ("Porto - Remote", "Porto,
/// Portugal (Remote)"), and whether it had one. A value that is only a way of
/// working is returned empty.
fn without_work_mode(value: &str) -> (&str, bool) {
    if WORK_MODES.contains(&plain(value).as_str()) {
        return ("", true);
    }
    if let Some(open) = value.rfind('(')
        && value.ends_with(')')
        && WORK_MODES.contains(&plain(&value[open + 1..value.len() - 1]).as_str())
    {
        return (value[..open].trim_end(), true);
    }
    for separator in [" - ", " – ", " — ", " · ", " | "] {
        if let Some((place, mode)) = value.rsplit_once(separator)
            && WORK_MODES.contains(&plain(mode).as_str())
        {
            return (place.trim_end(), true);
        }
    }
    (value, false)
}

/// Whether a full stop in `value` ends a sentence, not an abbreviation ("St.
/// Gallen"): one after a word of more than three letters, or one that is
/// not followed by a space.
fn has_sentence_stop(value: &str) -> bool {
    value.split_whitespace().any(|word| {
        word.strip_suffix('.').map_or(word.contains('.'), |stem| {
            stem.contains('.') || stem.chars().count() > 3
        })
    })
}

/// Whether `part` reads like a place's name: up to five words, the first
/// capitalised, each capitalised or a joining word, none that says what
/// someone did, and no technology.
fn reads_like_a_name(part: &str) -> bool {
    let words: Vec<&str> = part.split_whitespace().collect();
    (1..=5).contains(&words.len())
        && words
            .first()
            .and_then(|w| w.chars().next())
            .is_some_and(char::is_uppercase)
        && words.iter().all(|word| {
            let lower = plain(word);
            !NOT_A_PLACE.contains(&lower.as_str())
                && (word.chars().next().is_some_and(char::is_uppercase)
                    || CONNECTORS.contains(&lower.as_str())
                    || REGION_WORDS.contains(&lower.as_str()))
        })
        && !tech::mentions(part)
}

/// The parts of a place line, squashed ("Lisbon, Portugal": `lisbon`,
/// `portugal`).
fn parts_of(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(squash)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Whether `value` is a place rather than a description. The length, the
/// sentences and the line breaks are checked first; then a line with commas
/// is a place when it is the known `location`, or every part reads like a
/// name and the last is a country (or every part is a part of the location);
/// a line without one only when it is a part of the known location, a region,
/// a country, or a name with a way of working.
pub(crate) fn is_place(value: &str, location: Option<&str>) -> bool {
    let value = value.trim();
    if value.is_empty() || value.contains('\n') {
        return false;
    }
    let value = value.strip_suffix('.').unwrap_or(value).trim_end();
    let (value, with_mode) = without_work_mode(value);
    if value.is_empty() {
        return with_mode;
    }
    let commas = value.contains(',');
    if has_sentence_stop(value)
        || value
            .chars()
            .any(|c| c.is_ascii_digit() || "!?;:()@/\"|".contains(c))
        || value.chars().count() > if commas { MAX_PLACE_PARTS } else { MAX_PLACE }
    {
        return false;
    }
    let location = location.map(str::trim).filter(|l| !l.is_empty());
    let known = location.map(parts_of).unwrap_or_default();
    if location.is_some_and(|l| squash(l) == squash(value)) {
        return true;
    }
    if commas {
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        let named = (2..=4).contains(&parts.len()) && parts.iter().all(|p| reads_like_a_name(p));
        let country = parts
            .last()
            .is_some_and(|last| COUNTRIES.contains(&plain(last).as_str()));
        let inside = !known.is_empty() && parts_of(value).iter().all(|p| known.contains(p));
        return named && (country || inside);
    }
    if !reads_like_a_name(value) {
        return false;
    }
    let lower = plain(value);
    known.contains(&squash(value))
        || COUNTRIES.contains(&lower.as_str())
        || lower
            .split_whitespace()
            .any(|word| REGION_WORDS.contains(&word))
        || with_mode
}

/// Whether `a` and `b` name the same place: the same words, or one is a part
/// of the other ("Lisbon" and "Lisbon, Portugal").
pub(crate) fn same_place(a: &str, b: &str) -> bool {
    let (a, b) = (
        a.trim().trim_end_matches('.').trim(),
        b.trim().trim_end_matches('.').trim(),
    );
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let (pa, pb) = (parts_of(a), parts_of(b));
    squash(a) == squash(b) || pa.iter().all(|p| pb.contains(p)) || pb.iter().all(|p| pa.contains(p))
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

fn day_number(raw: &str) -> Option<usize> {
    (1..=2)
        .contains(&raw.len())
        .then(|| raw.parse::<usize>().ok())
        .flatten()
        .filter(|d| (1..=31).contains(d))
}

/// `value` without what follows it in brackets ("Present (1 year 4 months)").
fn without_brackets(value: &str) -> &str {
    value
        .split_once('(')
        .map_or(value, |(before, _)| before)
        .trim()
}

/// `value` as `YYYY`, `YYYY-MM` or `YYYY-MM-DD` when it is a date written
/// another common way (`Jul 2023`, `julho de 2023`, `07/2023`, `2023-7`,
/// `15/03/2021`, `15 Jul 2023`); as it is otherwise (the limits then drop
/// and count it). A day before a month is read the European way.
pub(crate) fn normalised_date(value: &str) -> String {
    let trimmed = without_brackets(value);
    let month = |year: u32, month: usize| format!("{year:04}-{month:02}");
    let day = |year: u32, month: usize, day: usize| format!("{year:04}-{month:02}-{day:02}");
    let numeric: Vec<&str> = trimmed.split(['-', '/', '.']).collect();
    match numeric.as_slice() {
        // 2023-07, 2023-7, 2023/07, 2023.07; 07/2023
        [first, second] => {
            if let (Some(year), Some(m)) = (year_of(first), month_number(second)) {
                return month(year, m);
            }
            if let (Some(m), Some(year)) = (month_number(first), year_of(second)) {
                return month(year, m);
            }
        }
        // 15/03/2021, 15.03.2021; 2021-03-15 is already right
        [first, second, third] => {
            if let (Some(d), Some(m), Some(year)) =
                (day_number(first), month_number(second), year_of(third))
            {
                return day(year, m, d);
            }
        }
        _ => {}
    }
    // Jul 2023, July 2023, julho de 2023, juil. 2023, Jul, 2023, 15 Jul 2023
    let words: Vec<String> = trimmed
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty() && !matches!(w.to_lowercase().as_str(), "de" | "of"))
        .map(str::to_owned)
        .collect();
    match words.as_slice() {
        [name, year] => {
            if let (Some(m), Some(year)) = (month_of(name), year_of(year)) {
                return month(year, m);
            }
        }
        [d, name, year] => {
            if let (Some(d), Some(m), Some(year)) = (day_number(d), month_of(name), year_of(year)) {
                return day(year, m, d);
            }
        }
        _ => {}
    }
    trimmed.to_owned()
}

/// Whether an end date says the position has not ended ("Present", "o
/// momento", "aujourd’hui", "Present (1 year 4 months)").
fn says_now(value: &str) -> bool {
    let said = plain(without_brackets(value));
    let said = said.trim_end_matches('.');
    NOW.contains(&said)
        || said
            .strip_prefix("até ")
            .or_else(|| said.strip_prefix("until "))
            .is_some_and(|rest| NOW.contains(&rest))
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
    /// Summaries that were a place, or began with one.
    pub(crate) places: u32,
    /// Summaries that only said the title or the company again.
    pub(crate) titles: u32,
    /// Bullet lines taken out of a summary as highlights.
    pub(crate) bullets: u32,
}

/// `place` as the position's location when it has none.
fn place_into(work: &mut Work, place: &str) {
    if work.location.as_deref().is_none_or(|l| l.trim().is_empty()) {
        work.location = Some(place.trim().trim_end_matches('.').to_owned());
    }
}

/// One position put right.
fn repair_work(work: &mut Work, repairs: &mut Repairs) {
    work.start_date = date(work.start_date.take());
    work.end_date = date(work.end_date.take());
    if let Some(summary) = work.summary.take() {
        let summary = summary.trim().to_owned();
        let (first, rest) = summary
            .split_once('\n')
            .map_or((summary.as_str(), ""), |(first, rest)| {
                (first.trim(), rest.trim())
            });
        // The title said again first: "Staff Engineer" would read as a place.
        let rest = if repeats_title(&summary, work) {
            repairs.titles += 1;
            String::new()
        } else if is_place(first, work.location.as_deref()) {
            // A place, alone or on the first line of the description.
            repairs.places += 1;
            place_into(work, first);
            rest.to_owned()
        } else {
            summary.clone()
        };
        let (rest, bullets) = split_bullets(&rest);
        repairs.bullets += u32::try_from(bullets.len()).unwrap_or(u32::MAX);
        for bullet in bullets {
            add_highlight(&mut work.highlights, bullet);
        }
        work.summary = rest;
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
            "Coimbra, Coimbra, Portugal",
            "Porto, Portugal",
            "Lisboa, Portugal.",
            "Porto, Portugal (Remote)",
            "Porto - Remote",
            "Remote",
            "Lisboa e região",
            "Antuérpia e Região",
            "Greater Example Area",
            "Área Metropolitana de Lisboa",
            "Portugal",
            "Frankfurt am Main, Germany",
            "Région de Bruxelles-Capitale, Belgique",
            "St. Gallen, Switzerland",
            "Washington, District of Columbia, United States",
        ] {
            assert!(is_place(place, None), "{place}");
        }
    }

    #[test]
    fn descriptions_titles_and_tools_are_not_places() {
        for (not, location) in [
            (
                "Sold to retailers in Spain, Portugal, France",
                Some("Portugal"),
            ),
            ("Sold to retailers in Spain, Portugal, France", None),
            ("Head of Engineering", None),
            ("Head of Engineering", Some("Porto, Portugal")),
            ("Customer Success", None),
            ("Rust, Kafka, PostgreSQL", None),
            ("Rust, Kafka, PostgreSQL", Some("Porto, Portugal")),
            ("Sales, Marketing", None),
            ("Built the billing platform.", None),
            ("Led the platform team", None),
            ("Led Engineering", None),
            ("Full-time", None),
            ("Freelance", None),
            ("Jul 2023 - Apr 2024 (10 months)", None),
            ("I build boring, reliable backends for small teams.", None),
            ("Responsible for payments, invoicing and the team", None),
            ("Porto, Portugal\nBuilt the billing platform.", None),
            (
                "Built the billing platform for shops in Lisbon, Portugal and Spain.",
                Some("Lisbon, Portugal"),
            ),
            // No comma and nothing that says it is a place.
            ("Lisbon", None),
            ("", None),
        ] {
            assert!(!is_place(not, location), "{not} / {location:?}");
        }
    }

    #[test]
    fn the_known_location_makes_a_place() {
        assert!(is_place("lisbon,  portugal", Some("Lisbon, Portugal")));
        assert!(is_place("Lisbon", Some("Lisbon, Portugal")));
        assert!(is_place(
            "Lisbon, Lisbon District",
            Some("Lisbon, Lisbon District, Exampleland")
        ));
        assert!(!is_place("Porto", Some("Lisbon, Portugal")));
    }

    #[test]
    fn a_place_in_the_summary_moves_to_the_location() {
        let (draft, repairs) = repaired(Profile {
            work: vec![
                position("Coimbra, Coimbra, Portugal", None),
                position("Lisbon", Some("Lisbon, Portugal")),
                position("Built the billing platform.", Some("Porto, Portugal")),
                // On the first line of the description.
                position("Porto, Portugal\nBuilt the billing platform.", None),
                position(
                    "Lisboa e região\nRan the payments team.\n• Cut refunds by half",
                    None,
                ),
            ],
            ..Profile::default()
        });
        assert_eq!(draft.work[0].summary, None);
        assert_eq!(
            draft.work[0].location.as_deref(),
            Some("Coimbra, Coimbra, Portugal")
        );
        assert_eq!(draft.work[1].summary, None, "never a place in a summary");
        assert_eq!(draft.work[1].location.as_deref(), Some("Lisbon, Portugal"));
        assert_eq!(
            draft.work[2].summary.as_deref(),
            Some("Built the billing platform.")
        );
        assert_eq!(
            draft.work[3].summary.as_deref(),
            Some("Built the billing platform.")
        );
        assert_eq!(draft.work[3].location.as_deref(), Some("Porto, Portugal"));
        assert_eq!(
            draft.work[4].summary.as_deref(),
            Some("Ran the payments team.")
        );
        assert_eq!(draft.work[4].location.as_deref(), Some("Lisboa e região"));
        assert_eq!(draft.work[4].highlights, ["Cut refunds by half"]);
        assert_eq!(repairs.places, 4);
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
            ("mai 2021", "2021-05"),
            ("set 2021", "2021-09"),
            ("out 2021", "2021-10"),
            ("dez 2021", "2021-12"),
            ("07/2023", "2023-07"),
            ("7/2023", "2023-07"),
            ("2023-7", "2023-07"),
            ("2023/07", "2023-07"),
            ("15/03/2021", "2021-03-15"),
            ("15 Jul 2023", "2023-07-15"),
            ("Jul 2023 (1 year 4 months)", "2023-07"),
            ("2023", "2023"),
            ("2023-07", "2023-07"),
            ("2023-07-15", "2023-07-15"),
            ("sometime", "sometime"),
        ] {
            assert_eq!(normalised_date(written), want, "{written}");
        }
        for now in [
            "Present",
            "Present (1 year 4 months)",
            "o momento",
            "até o momento",
            "aujourd’hui",
            "aujourd'hui",
            "la actualidad",
        ] {
            assert!(says_now(now), "{now}");
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
