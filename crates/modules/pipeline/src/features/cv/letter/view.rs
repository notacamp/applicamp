//! What the letter's review page is shown: the letter with each paragraph's
//! tags and sources, each sentence's reasons, the job's questions and the
//! counts. All of it is computed from the stored draft; none of it calls a
//! model.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use super::{
    check::{Checks, LetterReason},
    model::{Answer, Content, LetterSize, Paragraph, Sentence},
    pdf::{closing, greeting},
};
use crate::features::cv::{
    check::claims_of,
    model::{Frame, SourceKind},
    pdf::{data::lang_of, readback::PdfChecks},
    view::{ContactView, DraftStatus, LinkView, SourceView, role_of, to_u32},
};

/// What a paragraph or an answer rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LetterTag {
    /// It cites a fact read on the company's own site.
    CompanyFact,
    /// It cites the person's profile.
    FromProfile,
    /// It cites nothing: it says nothing about the person that needs a source.
    NoClaims,
}

/// A sentence of the letter.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LetterSentenceView {
    pub id: String,
    pub text: String,
    /// The person wrote it.
    pub edited: bool,
    /// The sources it cites that the frame has.
    pub sources: Vec<SourceView>,
    /// Why it cannot be traced; empty when it can.
    pub flags: Vec<LetterReason>,
    /// The words of the sentence the check looks for in its sources.
    pub claims: Vec<String>,
}

/// A paragraph of the letter.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LetterParagraphView {
    pub id: String,
    pub tags: Vec<LetterTag>,
    /// "Where this came from": every source its sentences cite, once.
    pub sources: Vec<SourceView>,
    pub sentences: Vec<LetterSentenceView>,
}

/// An answer to one of the job's questions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LetterAnswerView {
    pub id: String,
    pub question: String,
    /// Empty when the profile can't support one.
    pub text: String,
    /// The person wrote it.
    pub own: bool,
    pub tags: Vec<LetterTag>,
    pub sources: Vec<SourceView>,
    pub flags: Vec<LetterReason>,
}

/// A draft, as the review page shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LetterDraftView {
    /// Its number for this opportunity, from 1.
    pub n: u32,
    pub status: DraftStatus,
    /// Once approved: the version, from 1.
    pub version: Option<u32>,
    pub language: String,
    pub size: LetterSize,
    /// The version of the approved CV it was written next to.
    pub cv_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    /// When it is deleted if nobody approves it.
    pub expires_at: Option<DateTime<Utc>>,
    pub contact: ContactView,
    /// The line above the letter, from the server.
    pub greeting: String,
    /// The line above the name, from the server.
    pub closing: String,
    pub paragraphs: Vec<LetterParagraphView>,
    /// The job's own questions and their answers; not in the PDF.
    pub answers: Vec<LetterAnswerView>,
    /// Words in the paragraphs.
    pub words: u32,
    /// The facts from the company's site the letter cites.
    pub company_facts: Vec<SourceView>,
    /// How many sentences and answers need the person.
    pub flagged: u32,
    /// The first of them, as the page shows them.
    pub first_flagged: Option<String>,
    /// Approve is on: nothing is open and it is still a draft.
    pub can_approve: bool,
}

/// An approved version: the approved letter as a PDF.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LetterVersion {
    /// The draft it renders.
    pub n: u32,
    pub version: u32,
    pub language: String,
    pub approved_at: DateTime<Utc>,
    /// The pages it takes (always 1).
    pub pages: u32,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub downloaded_at: Option<DateTime<Utc>>,
    /// The newest version of the opportunity.
    pub current: bool,
    /// What the read-back found.
    pub checks: PdfChecks,
}

/// What `view` is made from.
pub struct DraftParts<'a> {
    pub n: u32,
    pub status: DraftStatus,
    pub version: Option<u32>,
    pub language: &'a str,
    pub size: LetterSize,
    pub cv_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub content: &'a Content,
    pub frame: &'a Frame,
    pub checks: &'a Checks,
}

fn source_view(frame: &Frame, id: &str) -> Option<SourceView> {
    let s = frame.get(id)?;
    Some(SourceView {
        id: s.id.clone(),
        kind: s.kind,
        text: s.text.clone(),
        position: s.position.map(to_u32),
        role: s
            .position
            .and_then(|p| frame.position(p))
            .map(role_of)
            .filter(|r| !r.is_empty()),
        url: s.url.clone(),
    })
}

fn sources_of(frame: &Frame, ids: &[String]) -> Vec<SourceView> {
    let mut out: Vec<SourceView> = Vec::new();
    for view in ids.iter().filter_map(|id| source_view(frame, id)) {
        if !out.iter().any(|v| v.id == view.id) {
            out.push(view);
        }
    }
    out
}

fn tags_of(sources: &[SourceView]) -> Vec<LetterTag> {
    let mut tags = Vec::new();
    if sources.iter().any(|s| s.kind == SourceKind::Company) {
        tags.push(LetterTag::CompanyFact);
    }
    if sources.iter().any(|s| s.kind != SourceKind::Company) {
        tags.push(LetterTag::FromProfile);
    }
    if tags.is_empty() {
        tags.push(LetterTag::NoClaims);
    }
    tags
}

fn sentence_view(sentence: &Sentence, frame: &Frame, checks: &Checks) -> LetterSentenceView {
    LetterSentenceView {
        id: sentence.id.clone(),
        text: sentence.text.clone(),
        edited: sentence.edited,
        sources: sources_of(frame, &sentence.sources),
        flags: checks.get(&sentence.id).cloned().unwrap_or_default(),
        claims: claims_of(&sentence.text),
    }
}

fn paragraph_view(paragraph: &Paragraph, frame: &Frame, checks: &Checks) -> LetterParagraphView {
    let cited: Vec<String> = paragraph
        .sentences
        .iter()
        .flat_map(|s| s.sources.iter().cloned())
        .collect();
    let sources = sources_of(frame, &cited);
    LetterParagraphView {
        id: paragraph.id.clone(),
        tags: tags_of(&sources),
        sources,
        sentences: paragraph
            .sentences
            .iter()
            .map(|s| sentence_view(s, frame, checks))
            .collect(),
    }
}

fn answer_view(answer: &Answer, frame: &Frame, checks: &Checks) -> LetterAnswerView {
    let sources = sources_of(frame, &answer.sources);
    LetterAnswerView {
        id: answer.id.clone(),
        question: answer.question.clone(),
        text: answer.text.clone(),
        own: answer.own,
        tags: if answer.text.trim().is_empty() || answer.own {
            Vec::new()
        } else {
            tags_of(&sources)
        },
        sources,
        flags: checks.get(&answer.id).cloned().unwrap_or_default(),
    }
}

/// The draft the review page shows. `company` is the job's company, for the
/// greeting.
pub fn draft_view(parts: &DraftParts<'_>, company: &str) -> LetterDraftView {
    let content = parts.content;
    let frame = parts.frame;
    let checks = parts.checks;
    let paragraphs: Vec<LetterParagraphView> = content
        .paragraphs
        .iter()
        .map(|p| paragraph_view(p, frame, checks))
        .collect();
    let mut company_facts: Vec<SourceView> = Vec::new();
    for source in paragraphs.iter().flat_map(|p| p.sources.iter()) {
        if source.kind == SourceKind::Company && !company_facts.iter().any(|f| f.id == source.id) {
            company_facts.push(source.clone());
        }
    }
    // The page's order: the sentences, then the answers.
    let first_flagged = content
        .sentences()
        .map(|s| s.id.as_str())
        .chain(content.answers.iter().map(|a| a.id.as_str()))
        .find(|id| checks.contains_key(*id))
        .map(str::to_owned);
    let lang = lang_of(parts.language);
    LetterDraftView {
        n: parts.n,
        status: parts.status,
        version: parts.version,
        language: parts.language.to_owned(),
        size: parts.size,
        cv_version: parts.cv_version,
        created_at: parts.created_at,
        approved_at: parts.approved_at,
        expires_at: parts.expires_at,
        contact: ContactView {
            name: content.contact.name.clone(),
            email: content.contact.email.clone(),
            location: content.contact.location.clone(),
            url: content.contact.url.clone(),
            links: content
                .contact
                .links
                .iter()
                .map(|(network, url)| LinkView {
                    network: network.clone(),
                    url: url.clone(),
                })
                .collect(),
        },
        greeting: greeting(lang, company),
        closing: closing(lang).to_owned(),
        paragraphs,
        answers: content
            .answers
            .iter()
            .map(|a| answer_view(a, frame, checks))
            .collect(),
        words: to_u32(content.words()),
        company_facts,
        flagged: to_u32(checks.len()),
        first_flagged,
        can_approve: parts.status == DraftStatus::Draft && checks.is_empty(),
    }
}
