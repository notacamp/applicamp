//! A job, as every reader gives it: one shape whatever the board.
//!
//! Everything in it comes from the web and is untrusted: it is shown and
//! scored, never obeyed.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;

use crate::requirements::Salary;

/// Longest description kept, in characters.
pub const MAX_DESCRIPTION: usize = 20_000;

/// Where jobs are read from: a company's board on an applicant tracking
/// system (Greenhouse, Lever, Ashby), HN "Who is hiring", or a job board of
/// the registry ([`crate::job_boards`]), whose jobs are named after the
/// board ([`Job::source_name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// Greenhouse's public job board API.
    Greenhouse,
    /// Lever's public postings API.
    Lever,
    /// Ashby's public posting API.
    Ashby,
    /// Arbeitnow, as opportunities found before the job-board registry name
    /// it; its jobs are now read as the `arbeitnow` job board.
    Arbeitnow,
    /// HN "Who is hiring", through Algolia.
    Hn,
    /// A job board of the registry, read through its config.
    Board,
}

impl Source {
    /// Every source.
    pub const ALL: [Self; 6] = [
        Self::Greenhouse,
        Self::Lever,
        Self::Ashby,
        Self::Arbeitnow,
        Self::Hn,
        Self::Board,
    ];

    /// Lower-case name, as configured and stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever => "lever",
            Self::Ashby => "ashby",
            Self::Arbeitnow => "arbeitnow",
            Self::Hn => "hn",
            Self::Board => "board",
        }
    }

    /// A company board on an applicant tracking system.
    pub const fn is_company_board(self) -> bool {
        matches!(self, Self::Greenhouse | Self::Lever | Self::Ashby)
    }

    /// The source named `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == name)
    }
}

/// How a job is worked, when the board says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkMode {
    /// From anywhere they allow.
    Remote,
    /// Some days in an office.
    Hybrid,
    /// In an office.
    Onsite,
}

impl WorkMode {
    /// Lower-case name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Hybrid => "hybrid",
            Self::Onsite => "onsite",
        }
    }

    /// The work mode a board or a model names (`on-site`, `OnSite`,
    /// `remote`…); `None` for anything else (`unspecified`).
    pub fn parse(raw: &str) -> Option<Self> {
        let normal: String = raw
            .chars()
            .filter(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_ascii_lowercase();
        match normal.as_str() {
            "remote" => Some(Self::Remote),
            "hybrid" => Some(Self::Hybrid),
            "onsite" | "inoffice" | "office" => Some(Self::Onsite),
            _ => None,
        }
    }
}

/// Where a job was seen on a job board, with the posting's link there: the
/// app credits every one ("From Himalayas").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLink {
    /// The board's slug.
    pub board: String,
    /// The board's name, as its terms ask to credit it.
    pub name: String,
    /// The posting on that board.
    pub url: String,
}

/// What a job board of the registry says of a job, beyond what every
/// reader gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardJob {
    /// The board's slug: the job's source.
    pub slug: String,
    /// Every board it was seen on (this one first), each with its link.
    pub links: Vec<SourceLink>,
    /// The apply link exactly as the board gives it (never rewritten).
    pub apply_url: Option<String>,
    /// Who may apply, as the board's structured field says ("United
    /// States", "Europe", "Anywhere"); empty when the board does not say.
    pub regions: Vec<String>,
    /// When the posting expires, for a board whose terms ask to drop
    /// expired jobs.
    pub expires_at: Option<DateTime<Utc>>,
    /// The pay its structured fields state.
    pub pay: Option<Salary>,
}

/// A job read from a board.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// Where it was read.
    pub source: Source,
    /// Its id at the source, unique within it (`<board>:<id>` on Greenhouse,
    /// whose job pages need the board).
    pub source_ref: String,
    /// The role.
    pub title: String,
    /// The company.
    pub company: String,
    /// Where, as the board writes it.
    pub location: Option<String>,
    /// How it is worked, when the board says.
    pub work_mode: Option<WorkMode>,
    /// Whether a structured board field says it is remote: Ashby's
    /// `isRemote` / `workplaceType`, Lever's `workplaceType`. `None` where
    /// the board has no reliable field (Greenhouse, Arbeitnow, whose
    /// `remote` flag is ignored, HN): the model reads those.
    pub board_remote: Option<bool>,
    /// The posting's page.
    pub url: String,
    /// When it was published (or last updated, when that is all there is).
    pub posted_at: Option<DateTime<Utc>>,
    /// Pay, as the board writes it.
    pub salary_text: Option<String>,
    /// Plain text; empty until read for a board whose list leaves it out.
    pub description: String,
    /// What the job board of the registry it was read on says; `None` for a
    /// company board.
    pub board: Option<BoardJob>,
}

impl Job {
    /// Its source as stored: the job board's slug, else the reader's name.
    pub fn source_name(&self) -> &str {
        self.board
            .as_ref()
            .map_or(self.source.as_str(), |board| board.slug.as_str())
    }

    /// Its key: `(source, source_ref)`.
    pub fn key(&self) -> (String, String) {
        (self.source_name().to_owned(), self.source_ref.clone())
    }

    /// A free-text HN comment: the model reads its company and role.
    pub fn needs_reading(&self) -> bool {
        self.source == Source::Hn
    }
}

static SCRIPTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|style|noscript|template)\b.*?</(script|style|noscript|template)\s*>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<br\s*/?>|</(p|div|ul|ol|h[1-6]|tr|section|article|blockquote)\s*>|<p\b[^>]*>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static ITEMS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<li\b[^>]*>").unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<[^>]*>").unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static ENTITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// `text` with HTML character references decoded (`&amp;`, `&#39;`,
/// `&#x2019;`, the common named ones).
pub fn decode_entities(text: &str) -> String {
    ENTITY
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let name = caps.get(1).map_or("", |m| m.as_str());
            let decoded =
                if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse().ok().and_then(char::from_u32)
                } else {
                    match name {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        "nbsp" => Some(' '),
                        "ndash" => Some('–'),
                        "mdash" => Some('—'),
                        "hellip" => Some('…'),
                        "rsquo" => Some('’'),
                        "lsquo" => Some('‘'),
                        "rdquo" => Some('”'),
                        "ldquo" => Some('“'),
                        "euro" => Some('€'),
                        "pound" => Some('£'),
                        "bull" => Some('•'),
                        "middot" => Some('·'),
                        _ => None,
                    }
                };
            decoded.map_or_else(|| caps[0].to_owned(), String::from)
        })
        .into_owned()
}

/// HTML as plain text: scripts and styles dropped with their content,
/// paragraphs and items on their own lines, every other tag removed,
/// characters decoded, blank runs collapsed, at most
/// [`MAX_DESCRIPTION`] characters.
pub fn html_to_text(html: &str) -> String {
    let text = SCRIPTS.replace_all(html, " ");
    let text = ITEMS.replace_all(&text, "\n- ");
    let text = BREAKS.replace_all(&text, "\n");
    let text = TAGS.replace_all(&text, " ");
    let text = decode_entities(&text);
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() || line == "-" {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
        } else {
            lines.push(line);
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n").chars().take(MAX_DESCRIPTION).collect()
}

/// An RFC 3339 time (`2026-10-20T10:00:00-04:00`); `None` when it is not one.
pub fn when(raw: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw?.trim())
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// `text` trimmed with runs of blanks as one space; `None` when blank.
pub fn one_line(text: Option<&str>) -> Option<String> {
    let text = text?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_plain_text_without_scripts() {
        let html = "<p>We build <b>things</b> &amp; more.</p><script>alert('x')</script>\
                    <ul><li>Rust</li><li>Postgres&nbsp;17</li></ul><style>p{}</style>\
                    <p>Apply &#8212; now&#x21;</p>";
        assert_eq!(
            html_to_text(html),
            "We build things & more.\n\n- Rust\n- Postgres 17\n\nApply — now!"
        );
        assert!(!html_to_text("<SCRIPT type=x>steal()</SCRIPT>hi").contains("steal"));
    }

    #[test]
    fn escaped_html_decodes_in_two_steps() {
        let escaped = "&lt;p&gt;Hello &amp;amp; welcome&lt;/p&gt;";
        assert_eq!(html_to_text(&decode_entities(escaped)), "Hello & welcome");
    }

    #[test]
    fn work_modes_are_read_from_board_words() {
        assert_eq!(WorkMode::parse("on-site"), Some(WorkMode::Onsite));
        assert_eq!(WorkMode::parse("OnSite"), Some(WorkMode::Onsite));
        assert_eq!(WorkMode::parse("Remote"), Some(WorkMode::Remote));
        assert_eq!(WorkMode::parse("unspecified"), None);
    }
}
