//! Test fixtures, all invented: a LinkedIn data export of nobody (Alex Demo
//! at Example Org), built as a ZIP from CSV strings, and PDFs written with
//! `lopdf`.

use std::io::{Cursor, Write as _};

use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// The export's files, as LinkedIn names them, with columns in LinkedIn's
/// order and a couple of files nothing reads.
pub(crate) const LINKEDIN_EXPORT: [(&str, &[u8]); 10] = [
    (
        "Profile.csv",
        "First Name,Last Name,Maiden Name,Address,Birth Date,Headline,Summary,Industry,Zip Code,Geo Location,Twitter Handles,Websites,Instant Messengers\n\
         Alex,Demo,,,,Backend engineer · Rust and Postgres,\"Builds calm, boring backends.\",Software Development,,\"Lisbon, Portugal\",,[PORTFOLIO:https://demo.example.com],\n"
            .as_bytes(),
    ),
    (
        "Positions.csv",
        b"Company Name,Title,Description,Location,Started On,Finished On\n\
          Example Org,Senior Engineer,Runs the billing platform.,\"Lisbon, Portugal\",Mar 2021,\n\
          Sample Labs,Engineer,Built the data pipeline.,\"Porto, Portugal\",Jun 2017,Feb 2021\n",
    ),
    (
        "Education.csv",
        b"School Name,Start Date,End Date,Notes,Degree Name,Activities\n\
          Sample University,2012,2017,,Master's degree,\n",
    ),
    ("Skills.csv", b"Name\nRust\nPostgreSQL\nTypeScript\n"),
    (
        "Languages.csv",
        b"Name,Proficiency\n\
          Portuguese,Native or bilingual proficiency\n\
          English,Full professional proficiency\n",
    ),
    (
        "Certifications.csv",
        b"Name,Url,Authority,Started On,Finished On,License Number\n\
          Cloud Basics,https://academy.example.com/c/1,Example Academy,Jan 2020,,\n",
    ),
    (
        "Projects.csv",
        b"Title,Description,Url,Started On,Finished On\n\
          Open ledger,A small bookkeeping tool.,https://ledger.example.com,May 2019,Dec 2019\n",
    ),
    (
        "Email Addresses.csv",
        b"Email Address,Confirmed,Primary,Updated On\n\
          old.demo@example.com,Yes,No,1/2/18\n\
          alex.demo@example.com,Yes,Yes,3/4/21\n",
    ),
    (
        "Connections.csv",
        b"Notes:\n\"Not read.\"\n\nFirst Name,Last Name,Email Address\nSam,Sample,sam@example.com\n",
    ),
    ("messages.csv", b"FROM,TO,CONTENT\nx,y,not read\n"),
];

/// A ZIP of `files` (`(path, contents)`), deflated or stored.
pub(crate) fn zip_of(files: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (path, contents) in files {
        writer.start_file(*path, options).unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The invented export, zipped.
pub(crate) fn export_zip() -> Vec<u8> {
    zip_of(&LINKEDIN_EXPORT, CompressionMethod::Deflated)
}

/// The export with `files` added (another position, say).
pub(crate) fn export_zip_with(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut all: Vec<(&str, &[u8])> = LINKEDIN_EXPORT
        .iter()
        .copied()
        .filter(|(name, _)| !files.iter().any(|(added, _)| added == name))
        .collect();
    all.extend_from_slice(files);
    zip_of(&all, CompressionMethod::Deflated)
}

/// A one-page PDF saying `lines` (ASCII) in Helvetica; with no lines, a page
/// with no text at all, like a scan.
pub(crate) fn pdf_saying(lines: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let mut operations = Vec::new();
    if !lines.is_empty() {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new("Tf", vec!["F1".into(), 11.into()]));
        operations.push(Operation::new("Td", vec![72.into(), 760.into()]));
        for line in lines {
            operations.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
            operations.push(Operation::new("Td", vec![0.into(), (-16).into()]));
        }
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// An invented CV, one line per entry.
pub(crate) const CV_LINES: [&str; 9] = [
    "ALEX DEMO",
    "Backend engineer - Lisbon, Portugal - alex.demo@example.com",
    "EXPERIENCE",
    "Example Org - Senior Engineer - Mar 2021 to present",
    "Runs the billing platform.",
    "Sample Labs - Engineer - 06/2017 to 02/2021",
    "EDUCATION",
    "Sample University - Master's degree, 2012 to 2017",
    "SKILLS: Rust, PostgreSQL, TypeScript. LANGUAGES: Portuguese, English",
];
