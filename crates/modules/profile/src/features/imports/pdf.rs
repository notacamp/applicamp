//! Text out of a PDF (a LinkedIn profile saved as PDF, or a CV), in memory,
//! with a pure-Rust extractor. Scanned PDFs have no text to read: that is an
//! answer, not a guess (no OCR).

use std::time::Duration;

use applicamp_kernel::{Error, Result};

/// Most pages read.
pub const MAX_PAGES: usize = 30;
/// Longest extraction allowed before giving up on a file.
const TIMEOUT: Duration = Duration::from_secs(20);

fn refuse(code: &'static str, detail: &str) -> Error {
    Error::BadRequest(code, detail.to_owned())
}

/// The text of the PDF `bytes`. Extraction runs on a blocking thread; a
/// file that makes the extractor fail, panic or take too long is
/// `pdf_unreadable`.
pub async fn text_of(bytes: Vec<u8>) -> Result<String> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(refuse(
            "not_a_pdf",
            "This file is not a PDF. Upload a PDF, or paste the text instead.",
        ));
    }
    let work = tokio::task::spawn_blocking(move || extract(&bytes));
    match tokio::time::timeout(TIMEOUT, work).await {
        Ok(Ok(result)) => result,
        // The extractor panicked (it can, on a malformed file), or never
        // finished.
        Ok(Err(_)) | Err(_) => Err(unreadable()),
    }
}

fn unreadable() -> Error {
    refuse(
        "pdf_unreadable",
        "We couldn't read this PDF. Try another file, or paste the text instead.",
    )
}

fn extract(bytes: &[u8]) -> Result<String> {
    let mut doc = pdf_extract::Document::load_mem(bytes).map_err(|_| unreadable())?;
    // Many PDFs are "encrypted" with an empty password (permissions only).
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        return Err(refuse(
            "pdf_unreadable",
            "This PDF is password-protected. Remove the password, or paste the text instead.",
        ));
    }
    if doc.get_pages().len() > MAX_PAGES {
        return Err(refuse(
            "pdf_too_many_pages",
            &format!("This PDF has more than {MAX_PAGES} pages."),
        ));
    }
    let mut text = String::new();
    {
        let mut output = pdf_extract::PlainTextOutput::new(&mut text);
        pdf_extract::output_doc(&doc, &mut output).map_err(|_| unreadable())?;
    }
    if text.trim().is_empty() {
        return Err(refuse(
            "pdf_no_text",
            "This PDF has no text we can read (it may be a scan). Paste the text instead.",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::imports::testkit::{CV_LINES, pdf_saying};

    #[tokio::test]
    async fn a_pdf_with_text_is_read() {
        let text = text_of(pdf_saying(&CV_LINES)).await.unwrap();
        let squashed: String = text.split_whitespace().collect();
        assert!(squashed.contains("ExampleOrg"), "{text}");
        assert!(squashed.contains("Runsthebillingplatform."), "{text}");
    }

    #[tokio::test]
    async fn a_scan_has_no_text() {
        let err = text_of(pdf_saying(&[])).await.unwrap_err();
        assert_eq!(err.code(), "pdf_no_text");
    }

    #[tokio::test]
    async fn other_files_are_refused() {
        assert_eq!(
            text_of(b"PK\x03\x04 a zip".to_vec())
                .await
                .unwrap_err()
                .code(),
            "not_a_pdf"
        );
        assert_eq!(
            text_of(b"%PDF-1.7\nnot really".to_vec())
                .await
                .unwrap_err()
                .code(),
            "pdf_unreadable"
        );
    }
}
