//! Places, without the model: whether a written place ("Lisbon, Portugal",
//! "Porto") is one a person named, and which countries it names.
//!
//! A small fixed table: country names in English and in their own and the
//! app's languages, and the bigger cities. A place it does not know names
//! no country; the requirement rules count that as unclear, so as not met.
//! Words that are not places ("Remote", "Anywhere", "Hybrid") never match.

/// The EU's members, as ISO 3166-1 alpha-2 codes.
pub const EU: [&str; 27] = [
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];

/// Country names, folded (lower case, no accents), by code.
const COUNTRIES: &[(&str, &[&str])] = &[
    ("AT", &["austria", "osterreich", "autriche"]),
    (
        "BE",
        &["belgium", "belgica", "belgique", "belgie", "belgien"],
    ),
    ("BG", &["bulgaria", "bulgarie", "bulgarien"]),
    ("CY", &["cyprus", "chipre", "chypre", "zypern"]),
    (
        "CZ",
        &[
            "czechia",
            "czech republic",
            "chequia",
            "tchequia",
            "tchequie",
            "tschechien",
            "cesko",
        ],
    ),
    (
        "DE",
        &[
            "germany",
            "deutschland",
            "alemanha",
            "alemania",
            "allemagne",
        ],
    ),
    (
        "DK",
        &["denmark", "danmark", "dinamarca", "danemark", "daenemark"],
    ),
    ("EE", &["estonia", "eesti", "estonie", "estland"]),
    ("ES", &["spain", "espana", "espanha", "espagne", "spanien"]),
    (
        "FI",
        &["finland", "suomi", "finlandia", "finlande", "finnland"],
    ),
    ("FR", &["france", "franca", "francia", "frankreich"]),
    (
        "GR",
        &["greece", "grecia", "grece", "griechenland", "hellas"],
    ),
    (
        "HR",
        &["croatia", "hrvatska", "croacia", "croatie", "kroatien"],
    ),
    (
        "HU",
        &["hungary", "magyarorszag", "hungria", "hongrie", "ungarn"],
    ),
    ("IE", &["ireland", "eire", "irlanda", "irlande", "irland"]),
    ("IT", &["italy", "italia", "italie", "italien"]),
    (
        "LT",
        &["lithuania", "lietuva", "lituania", "lituanie", "litauen"],
    ),
    ("LU", &["luxembourg", "luxemburgo", "luxemburg"]),
    (
        "LV",
        &["latvia", "latvija", "letonia", "lettonie", "lettland"],
    ),
    ("MT", &["malta", "malte"]),
    (
        "NL",
        &[
            "netherlands",
            "the netherlands",
            "holland",
            "nederland",
            "paises baixos",
            "paises bajos",
            "pays bas",
            "niederlande",
        ],
    ),
    ("PL", &["poland", "polska", "polonia", "pologne", "polen"]),
    ("PT", &["portugal"]),
    (
        "RO",
        &["romania", "roumanie", "rumanien", "rumania", "romenia"],
    ),
    ("SE", &["sweden", "sverige", "suecia", "suede", "schweden"]),
    (
        "SI",
        &[
            "slovenia",
            "slovenija",
            "eslovenia",
            "slovenie",
            "slowenien",
        ],
    ),
    (
        "SK",
        &[
            "slovakia",
            "slovensko",
            "eslovaquia",
            "slovaquie",
            "slowakei",
        ],
    ),
    (
        "GB",
        &[
            "united kingdom",
            "uk",
            "great britain",
            "britain",
            "england",
            "scotland",
            "wales",
            "northern ireland",
            "reino unido",
            "royaume uni",
            "grossbritannien",
        ],
    ),
    (
        "CH",
        &[
            "switzerland",
            "schweiz",
            "suisse",
            "svizzera",
            "suica",
            "suiza",
        ],
    ),
    ("NO", &["norway", "norge", "noruega", "norvege", "norwegen"]),
    ("IS", &["iceland", "islandia", "islande"]),
    (
        "US",
        &[
            "united states",
            "usa",
            "estados unidos",
            "etats unis",
            "vereinigte staaten",
        ],
    ),
    ("CA", &["canada"]),
    ("BR", &["brazil", "brasil", "bresil", "brasilien"]),
    ("MX", &["mexico", "mexique", "mexiko"]),
    ("AR", &["argentina", "argentine", "argentinien"]),
    ("IN", &["india", "inde", "indien"]),
    ("AU", &["australia", "australie", "australien"]),
    (
        "NZ",
        &[
            "new zealand",
            "nova zelandia",
            "nueva zelanda",
            "nouvelle zelande",
            "neuseeland",
        ],
    ),
    ("JP", &["japan", "japao", "japon"]),
    ("SG", &["singapore", "singapura", "singapur", "singapour"]),
    ("IL", &["israel"]),
    ("TR", &["turkey", "turkiye", "turquia", "turquie", "turkei"]),
    ("UA", &["ukraine", "ucrania", "ukraina"]),
    ("RS", &["serbia", "srbija", "servia", "serbie", "serbien"]),
    (
        "ZA",
        &[
            "south africa",
            "africa do sul",
            "sudafrica",
            "afrique du sud",
            "sudafrika",
        ],
    ),
    (
        "AE",
        &[
            "united arab emirates",
            "uae",
            "emirados arabes unidos",
            "emiratos arabes unidos",
        ],
    ),
];

/// The bigger cities, folded, by country.
const CITIES: &[(&str, &[&str])] = &[
    (
        "PT",
        &[
            "lisbon",
            "lisboa",
            "porto",
            "oporto",
            "braga",
            "coimbra",
            "aveiro",
            "faro",
            "funchal",
            "setubal",
            "leiria",
            "cascais",
            "oeiras",
            "sintra",
            "guimaraes",
            "evora",
            "viseu",
        ],
    ),
    (
        "ES",
        &[
            "madrid",
            "barcelona",
            "valencia",
            "seville",
            "sevilla",
            "malaga",
            "bilbao",
            "zaragoza",
            "palma",
            "alicante",
            "las palmas",
            "granada",
            "vigo",
            "a coruna",
        ],
    ),
    (
        "FR",
        &[
            "paris",
            "lyon",
            "marseille",
            "toulouse",
            "bordeaux",
            "lille",
            "nantes",
            "strasbourg",
            "montpellier",
            "rennes",
            "grenoble",
        ],
    ),
    (
        "DE",
        &[
            "berlin",
            "munich",
            "munchen",
            "hamburg",
            "frankfurt",
            "cologne",
            "koln",
            "stuttgart",
            "dusseldorf",
            "leipzig",
            "dresden",
            "hanover",
            "hannover",
            "nuremberg",
            "nurnberg",
            "bonn",
            "karlsruhe",
            "mannheim",
            "essen",
            "dortmund",
            "bremen",
        ],
    ),
    (
        "AT",
        &["vienna", "wien", "graz", "linz", "salzburg", "innsbruck"],
    ),
    (
        "CH",
        &[
            "zurich", "geneva", "geneve", "genf", "basel", "bern", "lausanne", "lucerne", "luzern",
            "zug",
        ],
    ),
    (
        "NL",
        &[
            "amsterdam",
            "rotterdam",
            "the hague",
            "den haag",
            "utrecht",
            "eindhoven",
            "groningen",
            "delft",
            "leiden",
        ],
    ),
    (
        "BE",
        &[
            "brussels",
            "bruxelles",
            "brussel",
            "antwerp",
            "antwerpen",
            "ghent",
            "gent",
            "leuven",
            "liege",
        ],
    ),
    (
        "IT",
        &[
            "rome", "roma", "milan", "milano", "turin", "torino", "naples", "napoli", "bologna",
            "florence", "firenze",
        ],
    ),
    ("IE", &["dublin", "cork", "galway", "limerick"]),
    (
        "GB",
        &[
            "london",
            "manchester",
            "edinburgh",
            "glasgow",
            "birmingham",
            "bristol",
            "leeds",
            "cambridge",
            "oxford",
            "belfast",
            "cardiff",
            "liverpool",
        ],
    ),
    (
        "PL",
        &[
            "warsaw", "warszawa", "krakow", "wroclaw", "gdansk", "poznan", "lodz",
        ],
    ),
    ("CZ", &["prague", "praha", "brno"]),
    ("HU", &["budapest"]),
    (
        "RO",
        &[
            "bucharest",
            "bucuresti",
            "cluj napoca",
            "cluj",
            "iasi",
            "timisoara",
        ],
    ),
    ("BG", &["sofia", "plovdiv"]),
    ("GR", &["athens", "athina", "thessaloniki"]),
    ("HR", &["zagreb"]),
    ("SI", &["ljubljana"]),
    ("SK", &["bratislava"]),
    (
        "SE",
        &["stockholm", "gothenburg", "goteborg", "malmo", "uppsala"],
    ),
    ("DK", &["copenhagen", "kobenhavn", "aarhus"]),
    ("FI", &["helsinki", "espoo", "tampere"]),
    ("NO", &["oslo", "bergen", "trondheim"]),
    ("EE", &["tallinn", "tartu"]),
    ("LV", &["riga"]),
    ("LT", &["vilnius", "kaunas"]),
    ("LU", &["luxembourg city"]),
    ("MT", &["valletta"]),
    ("CY", &["nicosia", "limassol"]),
    ("IS", &["reykjavik"]),
    (
        "US",
        &[
            "new york",
            "san francisco",
            "seattle",
            "boston",
            "austin",
            "chicago",
            "los angeles",
            "denver",
            "washington dc",
        ],
    ),
    ("CA", &["toronto", "vancouver", "montreal", "ottawa"]),
    (
        "BR",
        &[
            "sao paulo",
            "rio de janeiro",
            "belo horizonte",
            "florianopolis",
            "curitiba",
        ],
    ),
];

/// Words in a written place that are not places.
const NOT_PLACES: [&str; 12] = [
    "remote",
    "remoto",
    "remota",
    "teletrabajo",
    "teletravail",
    "anywhere",
    "worldwide",
    "global",
    "hybrid",
    "onsite",
    "office",
    "fully",
];

/// `c` without its accent, for the letters the table's languages use.
fn fold_char(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' | 'č' | 'ć' => 'c',
        'é' | 'è' | 'ê' | 'ë' | 'ě' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ñ' | 'ń' => 'n',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ű' => 'u',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ł' => 'l',
        'ř' => 'r',
        'ý' => 'y',
        other => other,
    }
}

/// The words of `text`: lower case, accents folded (`ß` as `ss`), split
/// on anything that is not a letter or a digit.
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('ß', "ss")
        .chars()
        .map(fold_char)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The words of `text` that may name a place.
fn place_words(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|w| !NOT_PLACES.contains(&w.as_str()))
        .collect()
}

/// Whether `haystack` holds `phrase`'s words, in a row.
fn holds(haystack: &[String], phrase: &[String]) -> bool {
    !phrase.is_empty()
        && haystack.len() >= phrase.len()
        && haystack
            .windows(phrase.len())
            .any(|window| window == phrase)
}

fn holds_name(haystack: &[String], name: &str) -> bool {
    holds(haystack, &words(name))
}

/// The countries `text` names, by country or by city, each once.
pub fn countries_in(text: &str) -> Vec<&'static str> {
    let haystack = place_words(text);
    let mut found: Vec<&'static str> = Vec::new();
    for (code, names) in COUNTRIES.iter().chain(CITIES) {
        if !found.contains(code) && names.iter().any(|name| holds_name(&haystack, name)) {
            found.push(*code);
        }
    }
    found
}

/// The country `text` is the name of, when it is only a country's name.
fn country_named(text: &str) -> Option<&'static str> {
    let haystack = place_words(text);
    COUNTRIES
        .iter()
        .find(|(_, names)| names.iter().any(|name| words(name) == haystack))
        .map(|(code, _)| *code)
}

/// Whether `code` is an EU member.
pub fn is_eu(code: &str) -> bool {
    EU.contains(&code)
}

/// Whether a job at `job` is at `wanted`, a place a person named: one names
/// the other ("Lisbon" and "Lisbon, Portugal"), or `wanted` is a country
/// and `job` is in it ("Portugal" and "Porto"). A city is not its whole
/// country: a job somewhere in "Portugal" is not one in "Lisbon".
pub fn same_place(job: &str, wanted: &str) -> bool {
    let (job_words, wanted_words) = (place_words(job), place_words(wanted));
    if holds(&job_words, &wanted_words) || holds(&wanted_words, &job_words) {
        return true;
    }
    country_named(wanted).is_some_and(|country| countries_in(job).contains(&country))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countries_are_read_from_names_and_cities() {
        assert_eq!(countries_in("Lisbon, Portugal"), ["PT"]);
        assert_eq!(countries_in("Porto"), ["PT"]);
        assert_eq!(countries_in("München"), ["DE"]);
        assert_eq!(countries_in("Remote - Österreich"), ["AT"]);
        assert_eq!(countries_in("Germany or Austria"), ["AT", "DE"]);
        assert!(countries_in("Remote").is_empty());
        assert!(countries_in("Somewhere Else").is_empty());
        assert!(is_eu("PT") && !is_eu("CH"));
    }

    #[test]
    fn places_match_by_name_and_country_never_by_remote() {
        assert!(same_place("Lisbon", "Lisbon, Portugal"));
        assert!(same_place("Lisboa, Portugal", "Lisboa"));
        assert!(same_place("Porto", "Portugal"));
        assert!(
            !same_place("Portugal", "Lisbon"),
            "a country is not its city"
        );
        assert!(!same_place("Berlin", "Lisbon"));
        assert!(!same_place("Remote - US", "Remote"));
        assert!(!same_place("Remote", "Lisbon"));
    }
}
