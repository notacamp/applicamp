//! The tailored CV as a PDF (#38): Typst compiled inside the server, then
//! the text read back out of the PDF and compared with the CV.
//!
//! - **Two templates** ([`Template`]): Clean and Compact, both a serif page
//!   with the standard headings in the CV's language, real text, real links,
//!   embedded fonts. Nothing is letter-spaced or uppercased, so a heading
//!   comes out of the text layer as the word it is.
//! - **The person's text is data, never markup**: it goes to the fixed,
//!   embedded template as JSON in `sys.inputs` and is shown as text
//!   ([`data`]). The compiler's world has no files, no packages, no network
//!   ([`world`]).
//! - **One or two pages** ([`Length`]): a CV that does not fit is not made.
//! - **Bounded**: the compile runs on a blocking thread with a timeout, at
//!   most two at once (a request waits a bounded time for its turn), and a
//!   document over 256 KB is refused.
//! - **Read back** ([`readback`]): every heading, every line, the keywords the
//!   review showed as on the CV, the name, the email and the links must come
//!   out of the PDF, or no version is made. No model. A draft's preview
//!   ([`preview`]) keeps what the read-back found as a note instead.
//! - **Why it failed** ([`Cause`]) is logged with its kind and the compiler's
//!   diagnostic counts, never the CV's text.

pub mod data;
pub mod readback;
mod world;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use typst::foundations::Smart;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use utoipa::ToSchema;

use self::{data::Prepared, readback::PdfChecks, world::CvWorld};
use super::model::{Content, Frame};

/// How long one compile may take.
const TIMEOUT: Duration = Duration::from_secs(10);
/// How long a render waits for its turn.
const SLOT_WAIT: Duration = Duration::from_secs(15);
/// How many compiles may run at once.
const CONCURRENT: usize = 2;

static RENDERS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(CONCURRENT)));

/// How the CV looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    /// Roomy: 10.5 pt, wide margins.
    #[default]
    Clean,
    /// Tighter: 9.5 pt, narrower margins, more lines to a page.
    Compact,
}

impl Template {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Compact => "compact",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "clean" => Some(Self::Clean),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }

    pub(crate) fn source(self) -> &'static str {
        match self {
            Self::Clean => include_str!("clean.typ"),
            Self::Compact => include_str!("compact.typ"),
        }
    }
}

/// How many pages the CV may take: one or two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    One,
    Two,
}

impl Length {
    pub const fn pages(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    pub const fn from_pages(pages: u8) -> Option<Self> {
        match pages {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            _ => None,
        }
    }
}

/// A PDF made and checked.
#[derive(Debug)]
pub struct Made {
    pub bytes: Vec<u8>,
    pub pages: u32,
    pub checks: PdfChecks,
}

/// Why a PDF could not be made now. Kinds and counts only: never the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The build has no fonts.
    NoFonts,
    /// The document is over its size.
    TooLarge,
    /// The compiler stopped, with these diagnostics.
    Compile {
        errors: usize,
        warnings: usize,
        hints: usize,
    },
    /// The PDF could not be written, with these diagnostics.
    Export { errors: usize },
    /// The PDF made could not be read back at all.
    Unreadable,
    /// Every render slot stayed busy past the wait.
    Busy,
    /// The compile took longer than its time.
    Timeout,
    /// The compile's thread failed.
    Crashed,
}

impl Cause {
    /// The kind, for logs.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NoFonts => "no_fonts",
            Self::TooLarge => "too_large",
            Self::Compile { .. } => "compile",
            Self::Export { .. } => "export",
            Self::Unreadable => "unreadable",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
            Self::Crashed => "crashed",
        }
    }

    /// Log it, for `what` (`cv`, `cv_preview`, `letter`): the kind and the
    /// compiler's counts.
    pub fn log(self, what: &'static str) {
        let (errors, warnings, hints) = match self {
            Self::Compile {
                errors,
                warnings,
                hints,
            } => (errors, warnings, hints),
            Self::Export { errors } => (errors, 0, 0),
            _ => (0, 0, 0),
        };
        tracing::warn!(
            document = what,
            cause = self.kind(),
            errors,
            warnings,
            hints,
            "a PDF could not be made"
        );
    }
}

/// Why no PDF was made.
#[derive(Debug)]
pub enum Refusal {
    /// It does not fit the length chosen.
    DoesntFit { pages: u32, length: u8 },
    /// It was made but the read-back found it lacking.
    Checks(Box<PdfChecks>),
    /// It could not be made now, and why.
    Unavailable(Cause),
}

/// The counts of `diagnostics` by severity: errors, warnings, hints.
fn counts(diagnostics: &[typst::diag::SourceDiagnostic]) -> (usize, usize, usize) {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == typst::diag::Severity::Error)
        .count();
    let hints = diagnostics.iter().map(|d| d.hints.len()).sum();
    (errors, diagnostics.len() - errors, hints)
}

/// The PDF's bytes and page count, or why not.
fn compile(source: &str, prepared: &Prepared, length: Length) -> Result<(Vec<u8>, u32), Refusal> {
    let world =
        CvWorld::new(source, prepared.json.clone()).ok_or(Refusal::Unavailable(Cause::Crashed))?;
    let compiled = typst::compile::<PagedDocument>(&world);
    let document = compiled.output.map_err(|diagnostics| {
        let (errors, warnings, hints) = counts(&diagnostics);
        let (_, also, _) = counts(&compiled.warnings);
        Refusal::Unavailable(Cause::Compile {
            errors,
            warnings: warnings + also,
            hints,
        })
    })?;
    let pages = u32::try_from(document.pages().len()).unwrap_or(u32::MAX);
    if pages > u32::from(length.pages()) {
        typst::comemo::evict(2);
        return Err(Refusal::DoesntFit {
            pages,
            length: length.pages(),
        });
    }
    let options = PdfOptions {
        creator: Smart::Custom(Some("Applicamp".to_owned())),
        ..PdfOptions::default()
    };
    let bytes = typst_pdf::pdf(&document, &options).map_err(|diagnostics| {
        Refusal::Unavailable(Cause::Export {
            errors: diagnostics.len(),
        })
    })?;
    typst::comemo::evict(2);
    Ok((bytes, pages))
}

/// Compile `prepared` and read the PDF back against what it must say. When
/// `strict`, a read-back that finds it lacking refuses it; else it is made
/// with what the read-back found.
fn build(
    source: &str,
    prepared: &Prepared,
    length: Length,
    keywords: &[String],
    strict: bool,
) -> Result<Made, Refusal> {
    if !world::fonts_ready() {
        return Err(Refusal::Unavailable(Cause::NoFonts));
    }
    let (bytes, pages) = compile(source, prepared, length)?;
    let extracted = readback::extract(&bytes).ok_or(Refusal::Unavailable(Cause::Unreadable))?;
    let checks = readback::check(
        &prepared.expect,
        keywords,
        &extracted,
        length.pages(),
        bytes.len(),
    );
    if strict && !checks.passed() {
        return Err(Refusal::Checks(Box::new(checks)));
    }
    Ok(Made {
        bytes,
        pages,
        checks,
    })
}

/// The CV `content`, set in `language` with `template` on at most `length`
/// pages, as a PDF that has been read back and found to say what the CV says.
/// `keywords` are the job keywords the review showed as on the CV.
pub async fn make(
    content: &Content,
    frame: &Frame,
    language: &str,
    template: Template,
    length: Length,
    keywords: Vec<String>,
) -> Result<Made, Refusal> {
    let prepared = data::prepare(content, frame, language)
        .map_err(|_| Refusal::Unavailable(Cause::TooLarge))?;
    make_prepared(template.source(), prepared, length, keywords, true).await
}

/// A draft's preview: as [`make`], but what the read-back finds is kept as a
/// note (in [`Made::checks`]) and never refuses it.
pub async fn preview(
    content: &Content,
    frame: &Frame,
    language: &str,
    template: Template,
    length: Length,
    keywords: Vec<String>,
) -> Result<Made, Refusal> {
    let prepared = data::prepare(content, frame, language)
        .map_err(|_| Refusal::Unavailable(Cause::TooLarge))?;
    make_prepared(template.source(), prepared, length, keywords, false).await
}

/// A document whose Typst `source` is fixed and embedded (a template of this
/// crate), given `prepared` as its only input: compiled in the same bounded,
/// file-less world, then read back against what `prepared` says it must hold
/// (refused when it lacks something and `strict`). A cover letter is made
/// this way.
pub(crate) async fn make_prepared(
    source: &'static str,
    prepared: Prepared,
    length: Length,
    keywords: Vec<String>,
    strict: bool,
) -> Result<Made, Refusal> {
    let permit = match tokio::time::timeout(SLOT_WAIT, Arc::clone(&RENDERS).acquire_owned()).await {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => return Err(Refusal::Unavailable(Cause::Crashed)),
        Err(_) => return Err(Refusal::Unavailable(Cause::Busy)),
    };
    // The permit goes with the work, so a compile that outlives its timeout
    // still counts as running until it is done.
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        build(source, &prepared, length, &keywords, strict)
    });
    match tokio::time::timeout(TIMEOUT, work).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(Refusal::Unavailable(Cause::Crashed)),
        Err(_) => Err(Refusal::Unavailable(Cause::Timeout)),
    }
}

#[cfg(test)]
mod tests;
