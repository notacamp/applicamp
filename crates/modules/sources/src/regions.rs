//! Who may apply, from a board's structured regions ("United States",
//! "Europe", "Anywhere", "LATAM"), without the model.
//!
//! The search reads remote jobs anywhere, as long as they are open to where
//! the person lives: a job whose regions are all understood and none of
//! them includes the person's country (by name, or as Europe, the EU or
//! EMEA for a European country, or worldwide) is not open to them. Regions
//! the table does not understand, or none at all, say nothing: the model
//! reads the posting.

use crate::places;

/// Words that open a job to everyone.
const WORLDWIDE: [&str; 6] = [
    "anywhere",
    "worldwide",
    "global",
    "world",
    "international",
    "everywhere",
];
/// Words that name Europe (or a region holding it).
const EUROPE: [&str; 6] = ["europe", "european", "eu", "emea", "eea", "schengen"];
/// Regions understood that are not Europe and name no country of the table.
const ELSEWHERE: [&str; 16] = [
    "latam",
    "latin america",
    "south america",
    "north america",
    "americas",
    "central america",
    "caribbean",
    "apac",
    "asia",
    "asia pacific",
    "africa",
    "middle east",
    "mena",
    "oceania",
    "us",
    "anz",
];

/// European countries (the EU, the EEA, Switzerland, the United Kingdom,
/// and the rest of the continent the place table knows).
pub const EUROPEAN: [&str; 34] = [
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", "GB", "CH", "NO", "IS", "UA",
    "RS", "TR",
];

/// What one region says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Region {
    worldwide: bool,
    europe: bool,
    countries: Vec<&'static str>,
    understood: bool,
}

fn holds(words: &[String], phrase: &str) -> bool {
    let phrase: Vec<String> = places::words(phrase);
    !phrase.is_empty()
        && words.len() >= phrase.len()
        && words.windows(phrase.len()).any(|w| w == phrase.as_slice())
}

fn region(text: &str) -> Region {
    let words = places::words(text);
    let worldwide =
        WORLDWIDE.iter().any(|w| holds(&words, w)) || (words.len() == 1 && words[0] == "remote");
    let europe = EUROPE.iter().any(|w| holds(&words, w));
    let mut countries = places::countries_in(text);
    if words.iter().any(|w| w == "us") && !countries.contains(&"US") {
        countries.push("US");
    }
    let elsewhere = ELSEWHERE.iter().any(|w| holds(&words, w));
    Region {
        understood: worldwide || europe || elsewhere || !countries.is_empty(),
        worldwide,
        europe,
        countries,
    }
}

/// Whether a job with `regions` is open to someone living in `country`
/// (ISO 3166-1): `Some(true)` when one region includes it, `Some(false)`
/// when every region is understood and none does, `None` when the regions
/// say nothing (none, or one the table does not know).
pub fn open_to(regions: &[String], country: &str) -> Option<bool> {
    if regions.is_empty() {
        return None;
    }
    let read: Vec<Region> = regions.iter().map(|r| region(r)).collect();
    let european = EUROPEAN.contains(&country);
    if read
        .iter()
        .any(|r| r.worldwide || r.countries.contains(&country) || (r.europe && european))
    {
        return Some(true);
    }
    if read.iter().all(|r| r.understood) {
        Some(false)
    } else {
        None
    }
}

/// Whether `regions` open a job to everyone.
pub fn worldwide(regions: &[String]) -> bool {
    regions.iter().any(|r| region(r).worldwide)
}

/// The country `place` is in, when the place table knows it.
pub fn country_of(place: &str) -> Option<&'static str> {
    places::countries_in(place).first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn regions_open_a_job_or_not() {
        assert_eq!(open_to(&r(&["United States"]), "PT"), Some(false));
        assert_eq!(open_to(&r(&["USA", "Canada"]), "PT"), Some(false));
        assert_eq!(open_to(&r(&["Germany"]), "PT"), Some(false));
        assert_eq!(
            open_to(&r(&["Germany", "Portugal", "Spain", "UK"]), "PT"),
            Some(true)
        );
        assert_eq!(open_to(&r(&["Europe"]), "PT"), Some(true));
        assert_eq!(open_to(&r(&["EMEA"]), "GB"), Some(true));
        assert_eq!(open_to(&r(&["Europe"]), "US"), Some(false));
        assert_eq!(open_to(&r(&["Anywhere"]), "BR"), Some(true));
        assert_eq!(open_to(&r(&["Worldwide"]), "PT"), Some(true));
        assert_eq!(open_to(&r(&["LATAM"]), "PT"), Some(false));
        assert_eq!(
            open_to(&r(&["USA, Canada, USA timezones"]), "PT"),
            Some(false)
        );
        // Nothing understood, or nothing at all: the model reads the posting.
        assert_eq!(open_to(&r(&["Uzbekistan"]), "PT"), None);
        assert_eq!(open_to(&r(&["Germany", "Atlantis"]), "PT"), None);
        assert_eq!(open_to(&[], "PT"), None);
        assert!(worldwide(&r(&["Anywhere"])));
        assert!(!worldwide(&r(&["Europe"])));
        assert_eq!(country_of("Lisbon"), Some("PT"));
    }
}
