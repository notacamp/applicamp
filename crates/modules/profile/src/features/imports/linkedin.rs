//! The LinkedIn data export: a ZIP of CSV files, read deterministically
//! (no model).
//!
//! The archive is checked before anything is extracted: at most
//! [`MAX_ENTRIES`] entries, none over [`MAX_ENTRY_BYTES`] and all together
//! at most [`MAX_TOTAL_BYTES`] once uncompressed, and no path that leaves
//! the archive (`..`, absolute). Only known CSVs are read, matched by file
//! name without case wherever they are in the archive; their columns are
//! matched by header name, in any order, with extra columns ignored. Nothing
//! touches the disk.

use std::io::{Cursor, Read as _};

use applicamp_kernel::{Error, Result};

use crate::schema::{
    Basics, Certificate, Education, Language, Link, Profile, Project, Skill, Work,
};

/// Most entries an archive may hold.
pub const MAX_ENTRIES: usize = 200;
/// Largest entry, uncompressed.
pub const MAX_ENTRY_BYTES: u64 = 5 * 1024 * 1024;
/// Largest archive, uncompressed.
pub const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

/// The files read, by lower-case file name.
const KNOWN: [&str; 8] = [
    "profile.csv",
    "positions.csv",
    "education.csv",
    "skills.csv",
    "languages.csv",
    "certifications.csv",
    "projects.csv",
    "email addresses.csv",
];

fn refuse(code: &'static str, detail: &str) -> Error {
    Error::BadRequest(code, detail.to_owned())
}

/// Whether `name` stays inside the archive: no `..` part, not absolute, no
/// drive, no NUL.
fn safe_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['/', '\\'])
        && !name.contains('\0')
        && !name
            .split(['/', '\\'])
            .any(|part| part == ".." || part.contains(':'))
}

/// The draft profile in the export `bytes`, or why the archive is refused.
pub fn read_export(bytes: &[u8]) -> Result<Profile> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| {
        refuse(
            "not_a_zip",
            "This file is not a ZIP archive. Upload the ZIP LinkedIn sent you.",
        )
    })?;
    if archive.len() > MAX_ENTRIES {
        return Err(refuse(
            "archive_too_many_entries",
            &format!("This archive holds more than {MAX_ENTRIES} files."),
        ));
    }
    // Every check before anything is extracted.
    let mut total: u64 = 0;
    let mut wanted = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| refuse("not_a_zip", "This ZIP archive is damaged."))?;
        let name = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        if !safe_path(&name) || entry.enclosed_name().is_none() {
            return Err(refuse(
                "archive_unsafe_path",
                "This archive has a file whose path leaves the archive.",
            ));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(refuse(
                "archive_entry_too_large",
                "A file in this archive is over 5 MB once uncompressed.",
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err(refuse(
                "archive_too_large",
                "This archive is over 50 MB once uncompressed.",
            ));
        }
        let file_name = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if entry.is_file() && KNOWN.contains(&file_name.as_str()) {
            wanted.push((index, file_name));
        }
    }
    let mut files = Vec::new();
    for (index, file_name) in wanted {
        let entry = archive
            .by_index(index)
            .map_err(|_| refuse("not_a_zip", "This ZIP archive is damaged."))?;
        // The declared size can lie: never read past the limit.
        let mut text = Vec::new();
        entry
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut text)
            .map_err(|_| refuse("not_a_zip", "This ZIP archive is damaged."))?;
        if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_ENTRY_BYTES {
            return Err(refuse(
                "archive_entry_too_large",
                "A file in this archive is over 5 MB once uncompressed.",
            ));
        }
        files.push((file_name, String::from_utf8_lossy(&text).into_owned()));
    }
    Ok(from_files(&files))
}

/// The profile in the export's CSV files (`(lower-case file name, text)`).
fn from_files(files: &[(String, String)]) -> Profile {
    let mut profile = Profile::default();
    for (name, text) in files {
        let table = Table::parse(text);
        match name.as_str() {
            "profile.csv" => read_basics(&table, &mut profile.basics),
            "positions.csv" => profile.work.extend(table.rows().map(|row| Work {
                name: row.get("Company Name"),
                position: row.get("Title"),
                location: row.get("Location"),
                start_date: row.date("Started On"),
                end_date: row.date("Finished On"),
                summary: row.get("Description"),
                highlights: Vec::new(),
                facts: Vec::new(),
            })),
            "education.csv" => profile.education.extend(table.rows().filter_map(|row| {
                Some(Education {
                    institution: row.get("School Name")?,
                    area: row.get("Field Of Study"),
                    study_type: row.get("Degree Name"),
                    start_date: row.date("Start Date"),
                    end_date: row.date("End Date"),
                })
            })),
            "skills.csv" => profile.skills.extend(table.rows().filter_map(|row| {
                Some(Skill {
                    name: row.get("Name")?,
                    level: None,
                })
            })),
            "languages.csv" => profile.languages.extend(table.rows().filter_map(|row| {
                Some(Language {
                    language: row.get("Name")?,
                    fluency: row.get("Proficiency"),
                })
            })),
            "certifications.csv" => profile.certificates.extend(table.rows().filter_map(|row| {
                Some(Certificate {
                    name: row.get("Name")?,
                    issuer: row.get("Authority"),
                    date: row.date("Started On"),
                    url: row.get("Url"),
                })
            })),
            "projects.csv" => profile.projects.extend(table.rows().filter_map(|row| {
                Some(Project {
                    name: row.get("Title")?,
                    description: row.get("Description"),
                    start_date: row.date("Started On"),
                    end_date: row.date("Finished On"),
                    url: row.get("Url"),
                })
            })),
            "email addresses.csv" => {
                let primary = table
                    .rows()
                    .find(|row| {
                        row.get("Primary")
                            .is_some_and(|p| p.eq_ignore_ascii_case("yes"))
                    })
                    .and_then(|row| row.get("Email Address"));
                if profile.basics.email.is_none() {
                    profile.basics.email = primary;
                }
            }
            _ => {}
        }
    }
    profile
}

fn read_basics(table: &Table, basics: &mut Basics) {
    let Some(row) = table.rows().next() else {
        return;
    };
    let name = [row.get("First Name"), row.get("Last Name")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        basics.name = Some(name);
    }
    basics.label = row.get("Headline");
    basics.summary = row.get("Summary");
    basics.location = row.get("Geo Location");
    basics.profiles = row
        .get("Websites")
        .map(|sites| urls(&sites))
        .unwrap_or_default()
        .into_iter()
        .map(|url| Link { network: None, url })
        .collect();
}

/// The `http(s)://` addresses in LinkedIn's `[PORTFOLIO:https://…,OTHER:…]`.
fn urls(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("http") {
        let tail = &rest[start..];
        let end = tail.find([',', ']', ' ', '\n']).unwrap_or(tail.len());
        let url = &tail[..end];
        if url.starts_with("https://") || url.starts_with("http://") {
            found.push(url.to_owned());
        }
        rest = &tail[end..];
    }
    found
}

/// A CSV file: its header and rows.
struct Table {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// Parse `text`. The header is the first of its first few lines that
    /// names a column (LinkedIn puts notes above some of them).
    fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());
        let records: Vec<Vec<String>> = reader
            .records()
            .filter_map(std::result::Result::ok)
            .map(|record| record.iter().map(|field| field.trim().to_owned()).collect())
            .collect();
        let start = records
            .iter()
            .take(5)
            .position(|record| record.iter().any(|field| is_known_column(field)))
            .unwrap_or(0);
        let mut records = records.into_iter().skip(start);
        let header = records.next().unwrap_or_default();
        Self {
            header,
            rows: records.collect(),
        }
    }

    fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        self.rows.iter().map(|cells| Row {
            header: &self.header,
            cells,
        })
    }
}

/// Every column name read, lower case.
fn is_known_column(field: &str) -> bool {
    const COLUMNS: [&str; 12] = [
        "first name",
        "headline",
        "company name",
        "title",
        "school name",
        "name",
        "proficiency",
        "authority",
        "email address",
        "started on",
        "degree name",
        "url",
    ];
    COLUMNS.contains(&field.to_lowercase().as_str())
}

/// One row, read by column name.
struct Row<'a> {
    header: &'a [String],
    cells: &'a [String],
}

impl Row<'_> {
    /// The cell under `column` (any case), if it has text.
    fn get(&self, column: &str) -> Option<String> {
        let index = self
            .header
            .iter()
            .position(|name| name.eq_ignore_ascii_case(column))?;
        self.cells
            .get(index)
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
            .map(str::to_owned)
    }

    /// The date under `column`, as `YYYY` or `YYYY-MM`.
    fn date(&self, column: &str) -> Option<String> {
        self.get(column).and_then(|text| parse_date(&text))
    }
}

/// `Jan 2020` → `2020-01`, `2020` → `2020`; ISO dates stay; anything else
/// is no date.
pub(crate) fn parse_date(text: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let text = text.trim();
    let is_year = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_digit());
    if is_year(text) {
        return Some(text.to_owned());
    }
    if crate::schema::fits(crate::schema::Kind::Date, text) {
        return Some(text.to_owned());
    }
    let mut words = text.split_whitespace();
    let (month, year) = (words.next()?, words.last()?);
    let month = month.to_lowercase();
    let number = MONTHS
        .iter()
        .position(|m| month.starts_with(m))
        .map(|i| i + 1)?;
    is_year(year).then(|| format!("{year}-{number:02}"))
}

#[cfg(test)]
mod tests {
    use zip::CompressionMethod;

    use super::*;
    use crate::features::imports::testkit::{export_zip as export, zip_of};

    #[test]
    fn an_export_becomes_a_profile() {
        let profile = read_export(&export()).unwrap();
        assert_eq!(profile.basics.name.as_deref(), Some("Alex Demo"));
        assert_eq!(
            profile.basics.label.as_deref(),
            Some("Backend engineer · Rust and Postgres")
        );
        assert_eq!(profile.basics.location.as_deref(), Some("Lisbon, Portugal"));
        assert_eq!(
            profile.basics.email.as_deref(),
            Some("alex.demo@example.com")
        );
        assert_eq!(profile.basics.profiles.len(), 1);
        assert_eq!(profile.basics.profiles[0].url, "https://demo.example.com");

        assert_eq!(profile.work.len(), 2);
        let first = &profile.work[0];
        assert_eq!(first.name.as_deref(), Some("Example Org"));
        assert_eq!(first.position.as_deref(), Some("Senior Engineer"));
        assert_eq!(first.start_date.as_deref(), Some("2021-03"));
        assert_eq!(first.end_date, None, "still there");
        assert_eq!(profile.work[1].end_date.as_deref(), Some("2021-02"));

        assert_eq!(profile.education.len(), 1);
        assert_eq!(profile.education[0].institution, "Sample University");
        assert_eq!(profile.education[0].start_date.as_deref(), Some("2012"));
        let skills: Vec<_> = profile.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skills, ["Rust", "PostgreSQL", "TypeScript"]);
        assert_eq!(profile.languages.len(), 2);
        assert_eq!(
            profile.languages[1].fluency.as_deref(),
            Some("Full professional proficiency")
        );
        assert_eq!(
            profile.certificates[0].issuer.as_deref(),
            Some("Example Academy")
        );
        assert_eq!(profile.projects[0].name, "Open ledger");
    }

    #[test]
    fn columns_are_read_by_name_in_any_order_and_case() {
        let csv = "\u{feff}Notes:\n\
            \"Exported for you.\"\n\
            Finished On,Extra,TITLE,company name,Started On\n\
            ,x,Engineer,Example Org,Jan 2020\n";
        let archive = zip_of(
            &[("Basic_LinkedInDataExport/POSITIONS.csv", csv.as_bytes())],
            CompressionMethod::Deflated,
        );
        let profile = read_export(&archive).unwrap();
        assert_eq!(profile.work.len(), 1);
        assert_eq!(profile.work[0].position.as_deref(), Some("Engineer"));
        assert_eq!(profile.work[0].name.as_deref(), Some("Example Org"));
        assert_eq!(profile.work[0].start_date.as_deref(), Some("2020-01"));
    }

    #[test]
    fn unknown_files_are_ignored() {
        let archive = zip_of(
            &[
                ("Connections.csv", b"First Name,Last Name\nSam,Sample\n"),
                ("messages.csv", b"FROM,CONTENT\nx,y\n"),
            ],
            CompressionMethod::Deflated,
        );
        assert!(read_export(&archive).unwrap().is_empty());
    }

    #[test]
    fn dates_in_linkedin_forms() {
        assert_eq!(parse_date("Jan 2020").as_deref(), Some("2020-01"));
        assert_eq!(parse_date("December 2019").as_deref(), Some("2019-12"));
        assert_eq!(parse_date("2020").as_deref(), Some("2020"));
        assert_eq!(parse_date("2020-05").as_deref(), Some("2020-05"));
        assert_eq!(parse_date("Present"), None);
        assert_eq!(parse_date(""), None);
    }

    #[test]
    fn not_a_zip_is_refused() {
        let err = read_export(b"%PDF-1.7 not a zip").unwrap_err();
        assert_eq!(err.code(), "not_a_zip");
    }

    #[test]
    fn more_than_200_entries_are_refused() {
        let names: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("file-{i}.txt")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        let err = read_export(&zip_of(&files, CompressionMethod::Stored)).unwrap_err();
        assert_eq!(err.code(), "archive_too_many_entries");

        // 200 is fine.
        let files = &files[..MAX_ENTRIES];
        assert!(read_export(&zip_of(files, CompressionMethod::Stored)).is_ok());
    }

    #[test]
    fn an_entry_over_5_mb_is_refused() {
        let big = vec![b'a'; usize::try_from(MAX_ENTRY_BYTES).unwrap() + 1];
        let archive = zip_of(&[("Skills.csv", &big)], CompressionMethod::Deflated);
        assert!(archive.len() < 1024 * 1024, "a small bomb");
        let err = read_export(&archive).unwrap_err();
        assert_eq!(err.code(), "archive_entry_too_large");
    }

    #[test]
    fn more_than_50_mb_in_all_is_refused() {
        let chunk = vec![b'a'; usize::try_from(MAX_ENTRY_BYTES).unwrap()];
        let names: Vec<String> = (0..11).map(|i| format!("part-{i}.bin")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &chunk[..])).collect();
        let err = read_export(&zip_of(&files, CompressionMethod::Stored)).unwrap_err();
        assert_eq!(err.code(), "archive_too_large");
    }

    #[test]
    fn a_path_leaving_the_archive_is_refused() {
        for path in [
            "../Positions.csv",
            "export/../../Positions.csv",
            "/etc/Positions.csv",
            "\\\\server\\Positions.csv",
            "C:/Positions.csv",
        ] {
            let archive = zip_of(&[(path, b"Title\nx\n")], CompressionMethod::Deflated);
            let err = read_export(&archive).unwrap_err();
            assert_eq!(err.code(), "archive_unsafe_path", "{path}");
        }
    }

    #[test]
    fn urls_are_found_in_the_websites_cell() {
        assert_eq!(
            urls("[PORTFOLIO:https://a.example.com,OTHER:http://b.example.com/x]"),
            ["https://a.example.com", "http://b.example.com/x"]
        );
        assert!(urls("none").is_empty());
    }
}
