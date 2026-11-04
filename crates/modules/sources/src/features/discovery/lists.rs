//! The public lists discovery reads for boards it does not know yet, each
//! with its licence and credit (the credit is kept as the board's "found
//! through"): Markdown lists with `-`, `*` or numbered items, a CSV that
//! names each board's feed, and Wikidata's "employment website" items.
//!
//! Only lists whose licence allows it are read (CC0 and MIT). A list gives
//! names and addresses, never jobs: every board it names is checked like any
//! other (robots rules, terms, feed, sample) before anything is read from
//! it.

use serde_json::Value;
use url::Url;

use super::ListEntry;

/// How a list is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `- [Name](https://…)` or `1. [Name](https://…)` under headings.
    Markdown,
    /// `Title,URL,Feed URL,Notes`.
    Csv,
    /// A SPARQL answer (`results.bindings`) with `url`, `itemLabel` and an
    /// optional `country` code.
    Wikidata,
}

/// A list discovery reads.
#[derive(Debug, Clone, Copy)]
pub struct ListSource {
    /// What `APPLICAMP_SOURCES__DISCOVERY_SOURCES` names it by.
    pub id: &'static str,
    /// How it is credited.
    pub name: &'static str,
    pub licence: &'static str,
    /// Where it is read (the first list's is configurable).
    pub url: &'static str,
    pub format: Format,
    /// Only the entries under these headings (all when empty).
    pub sections: &'static [&'static str],
}

impl ListSource {
    /// The words a board found through this list records.
    pub fn found_through(&self) -> String {
        format!("the public list {} ({})", self.name, self.licence)
    }
}

/// The id of the first list, whose address is configurable.
pub const FIRST: &str = "awesome-job-boards";

/// Every list, in the order they are read.
pub static SOURCES: [ListSource; 6] = [
    ListSource {
        id: FIRST,
        name: "awesome-job-boards by emredurukn",
        licence: "CC0",
        url: "https://raw.githubusercontent.com/emredurukn/awesome-job-boards/main/README.md",
        format: Format::Markdown,
        sections: &[],
    },
    ListSource {
        id: "remote-working-list",
        name: "remote-working-list by georgemandis",
        licence: "MIT",
        url: "https://raw.githubusercontent.com/georgemandis/remote-working-list/main/remote-working-resources.csv",
        format: Format::Csv,
        sections: &[],
    },
    ListSource {
        id: "awesome-remote-job",
        name: "awesome-remote-job by lukasz-madon",
        licence: "CC0",
        url: "https://raw.githubusercontent.com/lukasz-madon/awesome-remote-job/master/README.md",
        format: Format::Markdown,
        sections: &["Job boards", "Job boards aggregators"],
    },
    ListSource {
        id: "awesome-remote-jobs",
        name: "awesome-remote-jobs by alros",
        licence: "MIT",
        url: "https://raw.githubusercontent.com/alros/awesome-remote-jobs/main/README.md",
        format: Format::Markdown,
        sections: &["Most recommended", "Other websites"],
    },
    ListSource {
        id: "idimetrix-awesome-job-boards",
        name: "awesome-job-boards by idimetrix",
        licence: "MIT",
        url: "https://raw.githubusercontent.com/idimetrix/awesome-job-boards/main/README.md",
        format: Format::Markdown,
        sections: &[],
    },
    ListSource {
        id: "wikidata",
        name: "Wikidata employment websites",
        licence: "CC0",
        url: WIKIDATA,
        format: Format::Wikidata,
        sections: &[],
    },
];

/// Wikidata's employment websites (Q580148) with their official site and
/// country code, as a SPARQL query on its public endpoint.
const WIKIDATA: &str = "https://query.wikidata.org/sparql?format=json&query=SELECT%20DISTINCT%20%3Fitem%20%3FitemLabel%20%3Furl%20%3Fcountry%20WHERE%20%7B%20%3Fitem%20wdt%3AP31%2Fwdt%3AP279%2a%20wd%3AQ580148%3B%20wdt%3AP856%20%3Furl.%20OPTIONAL%20%7B%20%3Fitem%20wdt%3AP17%2Fwdt%3AP297%20%3Fcountry.%20%7D%20SERVICE%20wikibase%3Alabel%20%7B%20bd%3AserviceParam%20wikibase%3Alanguage%20%22en%22.%20%7D%20%7D%20LIMIT%20500";

/// Most entries read from one list.
const MAX_ENTRIES: usize = 2_000;

/// A list's text, as its entries: the boards it names.
pub fn entries(source: &ListSource, text: &str) -> Vec<ListEntry> {
    let mut entries = match source.format {
        Format::Markdown => super::parse_list(text),
        Format::Csv => parse_csv(text),
        Format::Wikidata => parse_wikidata(text),
    };
    if !source.sections.is_empty() {
        entries.retain(|entry| {
            source
                .sections
                .iter()
                .any(|section| entry.section.eq_ignore_ascii_case(section))
        });
    }
    entries.truncate(MAX_ENTRIES);
    entries
}

/// One CSV record's fields (quotes and doubled quotes read), `None` for a
/// blank line.
fn csv_record(line: &str) -> Option<Vec<String>> {
    if line.trim().is_empty() {
        return None;
    }
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    Some(fields)
}

/// An address from a list, `&amp;` read as `&`, only on the web.
fn web(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim().replace("&amp;", "&").as_str()).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()).then_some(url)
}

/// The boards of `Title,URL,Feed URL,Notes`: the feed address, when the
/// list has one, is a hint the check reads first.
pub fn parse_csv(text: &str) -> Vec<ListEntry> {
    text.lines()
        .skip(1)
        .filter_map(csv_record)
        .filter_map(|fields| {
            let name = fields.first()?.trim().to_owned();
            let url = web(fields.get(1)?)?;
            let feed = fields.get(2).and_then(|f| web(f));
            (!name.is_empty()).then(|| ListEntry {
                name,
                url,
                section: String::new(),
                feed,
                country: None,
            })
        })
        .collect()
}

/// The boards of a Wikidata SPARQL answer.
pub fn parse_wikidata(text: &str) -> Vec<ListEntry> {
    let Ok(answer) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let value = |binding: &Value, key: &str| -> Option<String> {
        binding.get(key)?.get("value")?.as_str().map(str::to_owned)
    };
    answer
        .pointer("/results/bindings")
        .and_then(Value::as_array)
        .map(|bindings| {
            bindings
                .iter()
                .filter_map(|binding| {
                    let url = web(&value(binding, "url")?)?;
                    let name = value(binding, "itemLabel")?;
                    let country = value(binding, "country")
                        .filter(|c| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase()));
                    Some(ListEntry {
                        name,
                        url,
                        section: String::new(),
                        feed: None,
                        country,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> &'static ListSource {
        SOURCES.iter().find(|s| s.id == id).unwrap()
    }

    #[test]
    fn numbered_lists_are_read_under_their_headings_only() {
        let markdown = "## Articles & Posts\n\n  1. [An article](https://blog.example/post)\n\n\
            ## Job boards\n\n  1. [Remote Board](https://remote.example/) - Remote jobs.\n  \
            1. [Other Board](https://other.example)\n\n## Job boards aggregators\n\n\
            1. [Aggregator](https://aggregator.example/jobs)\n\n## Housing\n\n\
            1. [Flat](https://flat.example)\n";
        let entries = entries(source("awesome-remote-job"), markdown);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Remote Board", "Other Board", "Aggregator"]);
        assert!(source("awesome-remote-job").found_through().contains("CC0"));
    }

    #[test]
    fn the_csv_gives_feeds_and_reads_quotes_and_escaped_addresses() {
        let csv = "Title,URL,Feed URL,Notes\n\
            Adafruit Jobs,https://jobs.example/?query=&amp;type=7,https://jobs.example/rss?query=&amp;type=7,\n\
            \"Quoted, Board\",https://quoted.example/,,\"a \"\"note\"\"\"\n\
            Broken,not a url,,\n\
            Feedless,https://feedless.example,,\n";
        let entries = parse_csv(csv);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[0].url.as_str(),
            "https://jobs.example/?query=&type=7"
        );
        assert_eq!(
            entries[0].feed.as_ref().map(Url::as_str),
            Some("https://jobs.example/rss?query=&type=7")
        );
        assert_eq!(entries[1].name, "Quoted, Board");
        assert!(entries[1].feed.is_none());
    }

    #[test]
    fn wikidata_answers_give_sites_and_countries() {
        let answer = r#"{"results": {"bindings": [
            {"url": {"value": "https://jobs.example/"}, "itemLabel": {"value": "Jobs Example"},
             "country": {"value": "DE"}},
            {"url": {"value": "ftp://old.example/"}, "itemLabel": {"value": "Old"}},
            {"url": {"value": "https://nocountry.example"}, "itemLabel": {"value": "No Country"}}
        ]}}"#;
        let entries = parse_wikidata(answer);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].country.as_deref(), Some("DE"));
        assert_eq!(entries[1].country, None);
        assert!(parse_wikidata("not json").is_empty());
    }
}
