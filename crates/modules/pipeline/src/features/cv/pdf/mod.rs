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
//!   most two at once, and a document over 256 KB is refused.
//! - **Read back** ([`readback`]): every heading, every line, the keywords the
//!   review showed as on the CV, the name, the email and the links must come
//!   out of the PDF, or no version is made. No model.

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

    fn source(self) -> &'static str {
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

/// Why no PDF was made.
#[derive(Debug)]
pub enum Refusal {
    /// It does not fit the length chosen.
    DoesntFit { pages: u32, length: u8 },
    /// It was made but the read-back found it lacking.
    Checks(Box<PdfChecks>),
    /// It could not be made now: too big, too slow, or the compiler failed.
    Unavailable,
}

/// The PDF's bytes and page count, or why not.
fn compile(
    template: Template,
    prepared: &Prepared,
    length: Length,
) -> Result<(Vec<u8>, u32), Refusal> {
    let world =
        CvWorld::new(template.source(), prepared.json.clone()).ok_or(Refusal::Unavailable)?;
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|_| Refusal::Unavailable)?;
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
    let bytes = typst_pdf::pdf(&document, &options).map_err(|_| Refusal::Unavailable)?;
    typst::comemo::evict(2);
    Ok((bytes, pages))
}

/// Compile `prepared` and read the PDF back against what it must say.
fn build(
    template: Template,
    prepared: &Prepared,
    length: Length,
    keywords: &[String],
) -> Result<Made, Refusal> {
    if !world::fonts_ready() {
        return Err(Refusal::Unavailable);
    }
    let (bytes, pages) = compile(template, prepared, length)?;
    let extracted = readback::extract(&bytes).ok_or(Refusal::Unavailable)?;
    let checks = readback::check(
        &prepared.expect,
        keywords,
        &extracted,
        length.pages(),
        bytes.len(),
    );
    if !checks.passed() {
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
    let prepared = data::prepare(content, frame, language).map_err(|_| Refusal::Unavailable)?;
    let permit = Arc::clone(&RENDERS)
        .acquire_owned()
        .await
        .map_err(|_| Refusal::Unavailable)?;
    // The permit goes with the work, so a compile that outlives its timeout
    // still counts as running until it is done.
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        build(template, &prepared, length, &keywords)
    });
    match tokio::time::timeout(TIMEOUT, work).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => {
            tracing::warn!("a CV PDF could not be made in time");
            Err(Refusal::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests;
