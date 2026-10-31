//! The read-back: the text is read out of the PDF again, the way an
//! applicant tracking system reads it, and compared with the CV. No model.
//!
//! A PDF that loses a heading ("EX PERIENCE" from letter-spaced type), a
//! line, the name or a link is refused, with why, and no version is made.

use std::collections::HashSet;

use pdf_extract::{Document, Object};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::data::Expect;
use crate::features::cv::view::{found, words};

/// What was read out of a PDF.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    /// Its text, in reading order.
    pub text: String,
    pub pages: u32,
    /// The places it links to (an email counts).
    pub link_targets: usize,
    /// Pictures in it: a CV has none, its text is text.
    pub images: usize,
}

/// Read the text, pages, links and pictures of `pdf`; none when it cannot be
/// read at all.
pub fn extract(pdf: &[u8]) -> Option<Extracted> {
    let text = pdf_extract::extract_text_from_mem(pdf).ok()?;
    let doc = Document::load_mem(pdf).ok()?;
    let pages = doc.get_pages();
    let mut link_targets = 0;
    for page in pages.values() {
        let Ok(annotations) = doc.get_page_annotations(*page) else {
            continue;
        };
        for annotation in annotations {
            let is_link = annotation
                .get(b"Subtype")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"Link");
            let action = annotation
                .get(b"A")
                .ok()
                .and_then(|a| doc.dereference(a).ok())
                .and_then(|(_, a)| a.as_dict().ok());
            if is_link && action.is_some_and(|a| a.has(b"URI")) {
                link_targets += 1;
            }
        }
    }
    let images = doc
        .objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"Image")
            })
        })
        .count();
    Some(Extracted {
        text,
        pages: u32::try_from(pages.len()).unwrap_or(u32::MAX),
        link_targets,
        images,
    })
}

/// `text` as it is compared: ligatures spelled out, no white space or soft
/// hyphen, plain quotes and dashes. Case is kept.
pub fn flat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            c if c.is_whitespace() => {}
            '\u{00ad}' | '\u{200b}' | '\u{feff}' => {}
            '\u{fb00}' => out.push_str("ff"),
            '\u{fb01}' => out.push_str("fi"),
            '\u{fb02}' => out.push_str("fl"),
            '\u{fb03}' => out.push_str("ffi"),
            '\u{fb04}' => out.push_str("ffl"),
            '\u{fb05}' | '\u{fb06}' => out.push_str("st"),
            '\u{2018}' | '\u{2019}' | '\u{201b}' | '\u{2032}' => out.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201f}' | '\u{2033}' => out.push('"'),
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            c => out.push(c),
        }
    }
    out
}

/// A line the PDF did not give back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MissingLine {
    /// The line (its id on the CV), or the head of a role or school.
    pub id: String,
    pub text: String,
}

/// The section headings of the CV against the PDF's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SectionsCheck {
    /// The headings the CV has.
    pub expected: Vec<String>,
    /// The ones the PDF's text does not have as whole words.
    pub missing: Vec<String>,
}

/// The job keywords the review showed as on the CV, against the PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct KeywordsCheck {
    /// How many were on the CV.
    pub total: u32,
    /// How many the PDF's text has.
    pub found: u32,
    pub missing: Vec<String>,
}

/// Every line of the CV against the PDF's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinesCheck {
    pub total: u32,
    pub missing: Vec<MissingLine>,
}

/// Name, email and links as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContactCheck {
    /// The name is text in the PDF (true when the CV has none).
    pub name: bool,
    /// The email is text in the PDF (true when the CV has none).
    pub email: bool,
    /// Every link is text and a real link in the PDF.
    pub links: bool,
}

/// The pages and the size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SizeCheck {
    pub pages: u32,
    /// The length the person chose, in pages.
    pub length: u8,
    pub bytes: u64,
}

/// What was checked on a PDF, kept with its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PdfChecks {
    pub sections: SectionsCheck,
    pub keywords: KeywordsCheck,
    pub lines: LinesCheck,
    /// There is text, and no picture: one text layer.
    pub text_layer: bool,
    pub contact: ContactCheck,
    pub size: SizeCheck,
}

impl PdfChecks {
    /// Whether everything passed.
    pub fn passed(&self) -> bool {
        self.sections.missing.is_empty()
            && self.keywords.missing.is_empty()
            && self.lines.missing.is_empty()
            && self.text_layer
            && self.contact.name
            && self.contact.email
            && self.contact.links
            && self.size.pages <= u32::from(self.size.length)
    }

    /// What failed, one plain sentence each.
    pub fn reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        for heading in &self.sections.missing {
            reasons.push(format!(
                "The heading “{heading}” is not readable in the PDF."
            ));
        }
        for keyword in &self.keywords.missing {
            reasons.push(format!(
                "The keyword “{keyword}” did not survive in the PDF."
            ));
        }
        for line in &self.lines.missing {
            reasons.push(format!(
                "This line did not survive in the PDF: “{}”",
                line.text
            ));
        }
        if !self.text_layer {
            reasons.push("The PDF has no readable text layer, or has text in pictures.".to_owned());
        }
        if !self.contact.name {
            reasons.push("The name is not readable in the PDF.".to_owned());
        }
        if !self.contact.email {
            reasons.push("The email is not readable in the PDF.".to_owned());
        }
        if !self.contact.links {
            reasons.push("A link is not readable, or not a real link, in the PDF.".to_owned());
        }
        if self.size.pages > u32::from(self.size.length) {
            reasons.push(format!(
                "It is {} pages long, not {}.",
                self.size.pages, self.size.length
            ));
        }
        reasons
    }
}

/// Whether `heading` is in `text` as a whole word, or words, on a line of its
/// own or inside a line (never as the end of a longer word, nor as letters
/// split by spaces).
fn has_heading(text: &str, heading: &str) -> bool {
    let words_of = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let wanted = words_of(heading);
    if wanted.is_empty() {
        return true;
    }
    // On a line of its own: how a heading is read.
    text.lines().any(|line| words_of(line) == wanted)
}

/// The PDF's `extracted` text, against what `expect` says it must hold.
pub fn check(
    expect: &Expect,
    keywords: &[String],
    extracted: &Extracted,
    length: u8,
    size: usize,
) -> PdfChecks {
    let squeezed = flat(&extracted.text);
    // A word broken at a line's end with a hyphen reads whole again.
    let joined = flat(&extracted.text.replace("-\n", ""));
    let has = |needle: &str| {
        let needle = flat(needle);
        squeezed.contains(&needle) || joined.contains(&needle)
    };

    let missing_headings: Vec<String> = expect
        .headings
        .iter()
        .filter(|h| !has_heading(&extracted.text, h))
        .cloned()
        .collect();

    let pdf_words: HashSet<String> = words(&extracted.text);
    let missing_keywords: Vec<String> = keywords
        .iter()
        .filter(|label| !found(label, &pdf_words))
        .cloned()
        .collect();

    let missing_lines: Vec<MissingLine> = expect
        .lines
        .iter()
        .filter(|line| !has(&line.text))
        .map(|line| MissingLine {
            id: line.id.clone(),
            text: line.text.clone(),
        })
        .collect();

    let links_as_text = expect.links.iter().all(|link| has(link));
    PdfChecks {
        sections: SectionsCheck {
            expected: expect.headings.clone(),
            missing: missing_headings,
        },
        keywords: KeywordsCheck {
            total: u32::try_from(keywords.len()).unwrap_or(u32::MAX),
            found: u32::try_from(keywords.len() - missing_keywords.len()).unwrap_or(u32::MAX),
            missing: missing_keywords,
        },
        lines: LinesCheck {
            total: u32::try_from(expect.lines.len()).unwrap_or(u32::MAX),
            missing: missing_lines,
        },
        text_layer: !extracted.text.trim().is_empty() && extracted.images == 0,
        contact: ContactCheck {
            name: expect.name.as_deref().is_none_or(&has),
            email: expect.email.as_deref().is_none_or(&has),
            links: links_as_text && extracted.link_targets >= expect.link_targets,
        },
        size: SizeCheck {
            pages: extracted.pages,
            length,
            bytes: u64::try_from(size).unwrap_or(u64::MAX),
        },
    }
}
