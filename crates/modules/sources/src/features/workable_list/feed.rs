//! Workable's list, scanned as it arrives (#88).
//!
//! The list is one XML file of every job Workable's customers publish, about
//! 860 MB. It is never held: [`Scanner::push`] takes each chunk of the
//! download, finds the `<job>…</job>` blocks it completes, hands over the
//! few fields the app uses (never the description) and forgets the block.
//! What it holds is at most one unfinished job, and [`MAX_JOB`] bounds that.
//! The format is the documented one: a `<source>` of `<job>`s, each field an
//! element holding text or CDATA.

use chrono::{DateTime, Utc};

/// The largest job block held while its end has not arrived; a block over
/// this is dropped (and counted).
pub const MAX_JOB: usize = 4 * 1024 * 1024;

const OPEN: &[u8] = b"<job>";
const CLOSE: &[u8] = b"</job>";

/// What the app reads of one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The company's name.
    pub company: String,
    /// The job's short code (`apply.workable.com/j/<code>`).
    pub code: String,
    /// The job's link, exactly as the list gives it.
    pub url: String,
    /// When it was posted, when the list says.
    pub date: Option<DateTime<Utc>>,
    /// The company's own site, when the list says.
    pub website: Option<String>,
}

/// Scans a download chunk by chunk.
#[derive(Debug, Default)]
pub struct Scanner {
    buf: Vec<u8>,
    /// Jobs found.
    pub jobs: u64,
    /// Blocks dropped: too large, or with no company or code.
    pub skipped: u64,
}

impl Scanner {
    /// A scanner with nothing read.
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the next `chunk` of the download; `on` is called with every job
    /// it completes.
    pub fn push(&mut self, chunk: &[u8], mut on: impl FnMut(Item)) {
        self.buf.extend_from_slice(chunk);
        while let Some(end) = find(&self.buf, CLOSE) {
            let block_end = end + CLOSE.len();
            let start = find(&self.buf[..end], OPEN);
            match start.and_then(|s| item(&self.buf[s + OPEN.len()..end])) {
                Some(found) => {
                    self.jobs += 1;
                    on(found);
                }
                None if start.is_some() => self.skipped += 1,
                None => {}
            }
            self.buf.drain(..block_end);
        }
        if self.buf.len() > MAX_JOB {
            // An unfinished block too large to be a job: drop what is held,
            // keeping a tail that may hold the start of a tag.
            self.skipped += 1;
            let keep = self.buf.len() - (CLOSE.len() - 1);
            self.buf.drain(..keep);
        } else if find(&self.buf, OPEN).is_none() && self.buf.len() > OPEN.len() {
            // Between jobs: only a possible start of `<job>` is worth keeping.
            let keep = self.buf.len() - (OPEN.len() - 1);
            self.buf.drain(..keep);
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// The text of the first `<tag>` in `block`.
fn field(block: &[u8], tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let at = find(block, open.as_bytes())? + open.len();
    let len = find(&block[at..], close.as_bytes())?;
    let raw = String::from_utf8_lossy(&block[at..at + len]);
    let raw = raw.trim();
    let text = match raw
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
    {
        Some(inner) => inner.trim().to_owned(),
        None => unescape(raw),
    };
    (!text.is_empty()).then_some(text)
}

fn unescape(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_owned();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let replaced = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let ch = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let number = name.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((ch, end))
        });
        match replaced {
            Some((ch, end)) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The list's date, `Wed, 28 Jul 2021 10:24:17 UTC`.
pub fn parse_date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    let raw = raw
        .strip_suffix(" UTC")
        .map_or_else(|| raw.to_owned(), |head| format!("{head} +0000"));
    DateTime::parse_from_rfc2822(&raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// The company's site: the `<website>` after the description, which may
/// itself hold anything.
fn website(block: &[u8]) -> Option<String> {
    let tail = rfind(block, b"</description>").map_or(block, |at| &block[at..]);
    field(tail, "website").filter(|w| w.len() <= 500)
}

/// The fields of one job block (what is between `<job>` and `</job>`).
fn item(block: &[u8]) -> Option<Item> {
    let company: String = field(block, "company")?.chars().take(300).collect();
    let url = field(block, "url").unwrap_or_default();
    let code = field(block, "referencenumber")
        .or_else(|| {
            url.rsplit('/')
                .find(|segment| !segment.is_empty())
                .map(str::to_owned)
        })?
        .trim()
        .to_owned();
    if code.is_empty() || code.len() > 40 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(Item {
        company,
        code,
        url,
        date: field(block, "date").and_then(|d| parse_date(&d)),
        website: website(block),
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    const FIXTURE: &str = include_str!("fixtures/workable.xml");

    fn scan(chunk: usize) -> (Vec<Item>, Scanner) {
        let mut scanner = Scanner::new();
        let mut items = Vec::new();
        for part in FIXTURE.as_bytes().chunks(chunk) {
            scanner.push(part, |i| items.push(i));
        }
        (items, scanner)
    }

    #[test]
    fn jobs_are_found_whatever_the_size_of_the_chunks() {
        let (whole, scanner) = scan(FIXTURE.len());
        assert_eq!(whole.len(), 4, "{whole:?}");
        assert_eq!(scanner.jobs, 4);
        assert_eq!(scanner.skipped, 1, "the job with no company is skipped");
        for size in [1, 2, 3, 7, 64, 1000] {
            let (parts, _) = scan(size);
            assert_eq!(parts, whole, "chunks of {size} bytes");
        }
    }

    #[test]
    fn a_job_is_read_as_the_list_gives_it() {
        let (items, _) = scan(100);
        let first = &items[0];
        assert_eq!(first.company, "Meridian Bakery & Co");
        assert_eq!(first.code, "AAAA111111");
        assert_eq!(first.url, "https://apply.workable.com/j/AAAA111111");
        assert_eq!(
            first.date,
            Some(Utc.with_ymd_and_hms(2026, 10, 30, 9, 15, 0).unwrap())
        );
        assert_eq!(
            first.website.as_deref(),
            Some("https://meridian-bakery.example.com")
        );
        // The company's site, not a link inside the description.
        assert_eq!(items[1].website, None);
        // A link with the token in it is kept as it is.
        assert_eq!(
            items[2].url,
            "https://apply.workable.com/lantern-press/j/DDDD444444/"
        );
        // Plain text (not CDATA) is unescaped.
        assert_eq!(items[3].company, "Harbour <Works>");
    }

    #[test]
    fn a_block_with_no_end_is_not_held_for_ever() {
        let mut scanner = Scanner::new();
        let mut found = 0;
        scanner.push(b"<job><title>x</title>", |_| found += 1);
        let filler = vec![b'a'; 1024 * 1024];
        for _ in 0..6 {
            scanner.push(&filler, |_| found += 1);
        }
        assert!(scanner.buf.len() < MAX_JOB + filler.len());
        assert_eq!(found, 0);
        assert!(scanner.skipped >= 1);
    }

    #[test]
    fn dates_parse_in_the_lists_form() {
        assert_eq!(
            parse_date("Wed, 28 Jul 2021 10:24:17 UTC"),
            Some(Utc.with_ymd_and_hms(2021, 7, 28, 10, 24, 17).unwrap())
        );
        assert_eq!(parse_date("yesterday"), None);
    }
}
