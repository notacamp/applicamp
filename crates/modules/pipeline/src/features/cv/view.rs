//! What the review page is shown: the draft with each line's marks, sources
//! and reasons, what was left out, the keyword panel and the counts. All of
//! it is computed from the stored draft; none of it calls a model.

use std::collections::HashSet;

use applicamp_kernel::{claims::squash, tech};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    check::{Checks, Reason, claims_of, flagged_in_order},
    model::{Content, Frame, Line, PositionInfo, SourceKind},
};
use crate::features::brief::BriefWant;

/// How a line relates to the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// Word for word what the profile says, in the place the profile has it.
    AsInProfile,
    /// Word for word, put earlier than the profile has it.
    MovedUp,
    /// The model reworded it from its sources.
    Reworded,
    /// The person wrote it.
    Edited,
}

/// A source a line cites.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SourceView {
    pub id: String,
    pub kind: SourceKind,
    /// Its words, as the profile (or the company's page) has them.
    pub text: String,
    /// The position it belongs to.
    pub position: Option<u32>,
    /// "Title at Company" of that position.
    pub role: Option<String>,
    /// For a company fact, the page it was read on.
    pub url: Option<String>,
}

/// A line of the CV.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LineView {
    pub id: String,
    pub text: String,
    pub mark: Mark,
    /// The sources it cites that the profile has.
    pub sources: Vec<SourceView>,
    /// Why it cannot be traced to the profile; empty when it can.
    pub flags: Vec<Reason>,
    /// The words of the line that the check looks for in its sources.
    pub claims: Vec<String>,
}

/// A position on the CV.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleView {
    pub position: u32,
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub bullets: Vec<LineView>,
}

/// A skill on the CV.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub flags: Vec<Reason>,
}

/// A link under the name.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LinkView {
    pub network: Option<String>,
    pub url: String,
}

/// How to reach the person, from the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContactView {
    pub name: Option<String>,
    pub email: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub links: Vec<LinkView>,
}

/// A school.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EducationView {
    pub institution: String,
    pub area: Option<String>,
    pub study_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// A language.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LanguageView {
    pub language: String,
    pub fluency: Option<String>,
}

/// A position that is not on the CV.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LeftOutRole {
    pub position: u32,
    pub title: Option<String>,
    pub company: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// How many lines of the profile it has.
    pub lines: u32,
}

/// A line of the profile, on a position that is on the CV, that is not.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LeftOutLine {
    /// The source id: what Put back is given.
    pub source: String,
    pub position: u32,
    pub title: Option<String>,
    pub company: Option<String>,
    pub text: String,
}

/// What the CV leaves out of the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LeftOut {
    pub roles: Vec<LeftOutRole>,
    pub lines: Vec<LeftOutLine>,
}

/// Where a job keyword is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeywordState {
    /// On the CV.
    OnCv,
    /// In the profile, not on the CV.
    InProfile,
    /// Not in the profile: never added for the person.
    NotInProfile,
}

/// Where a keyword comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeywordKind {
    /// What the job wants most (the brief's).
    Want,
    /// A technology the posting names.
    Tech,
}

/// A keyword of the job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Keyword {
    pub label: String,
    pub kind: KeywordKind,
    /// `must` or `nice` for a want.
    pub level: Option<String>,
    pub state: KeywordState,
}

/// The job's keywords against the CV and the profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Keywords {
    pub items: Vec<Keyword>,
    /// How many are on the CV.
    pub on_cv: u32,
    pub total: u32,
}

/// What changed from the profile.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct Changes {
    pub reworded: u32,
    pub as_in_profile: u32,
    pub moved_up: u32,
    pub edited: u32,
    pub left_out_roles: u32,
    pub left_out_lines: u32,
}

/// The status of a draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Draft,
    Approved,
    Discarded,
}

impl DraftStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Discarded => "discarded",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "approved" => Self::Approved,
            "discarded" => Self::Discarded,
            _ => Self::Draft,
        }
    }
}

/// A draft, as the review page shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DraftView {
    /// Its number for this opportunity, from 1.
    pub n: u32,
    pub status: DraftStatus,
    /// Once approved: the version, from 1.
    pub version: Option<u32>,
    /// The language it is written in.
    pub language: String,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    /// When it is deleted if nobody approves it.
    pub expires_at: Option<DateTime<Utc>>,
    pub contact: ContactView,
    pub headline: Option<LineView>,
    pub summary: Option<LineView>,
    pub experience: Vec<RoleView>,
    pub skills: Vec<SkillView>,
    pub education: Vec<EducationView>,
    pub languages: Vec<LanguageView>,
    pub left_out: LeftOut,
    /// How many lines (and skills) cannot be traced to the profile.
    pub flagged: u32,
    /// The first of them, as the page shows them.
    pub first_flagged: Option<String>,
    /// Approve is on: nothing is flagged and it is still a draft.
    pub can_approve: bool,
    pub keywords: Keywords,
    pub changes: Changes,
}

/// An approved version.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CvVersion {
    pub n: u32,
    pub version: u32,
    pub language: String,
    pub approved_at: DateTime<Utc>,
}

fn role_of(info: &PositionInfo) -> String {
    match (info.title.as_deref(), info.company.as_deref()) {
        (Some(title), Some(company)) => format!("{title} · {company}"),
        (Some(one), None) | (None, Some(one)) => one.to_owned(),
        (None, None) => String::new(),
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn mark_of(line: &Line, index: usize, frame: &Frame) -> Mark {
    if line.edited {
        return Mark::Edited;
    }
    if let [only] = line.sources.as_slice()
        && let Some(source) = frame.get(only)
        && squash(&source.text) == squash(&line.text)
    {
        // Earlier than the profile has it.
        return match source.order {
            Some(order) if index < order => Mark::MovedUp,
            _ => Mark::AsInProfile,
        };
    }
    Mark::Reworded
}

fn line_view(line: &Line, index: usize, frame: &Frame, checks: &Checks) -> LineView {
    LineView {
        id: line.id.clone(),
        text: line.text.clone(),
        mark: mark_of(line, index, frame),
        sources: line
            .sources
            .iter()
            .filter_map(|id| frame.get(id))
            .map(|s| SourceView {
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
            .collect(),
        flags: checks.get(&line.id).cloned().unwrap_or_default(),
        claims: claims_of(&line.text),
    }
}

/// The sources of the profile that are lines a person can put back, on the
/// positions `content` shows, that no line cites.
fn unused_lines<'a>(content: &Content, frame: &'a Frame) -> Vec<&'a super::model::Source> {
    let cited: HashSet<&str> = content
        .lines()
        .flat_map(|(l, _)| l.sources.iter().map(String::as_str))
        .collect();
    frame
        .sources
        .iter()
        .filter(|s| matches!(s.kind, SourceKind::Highlight | SourceKind::Fact))
        .filter(|s| {
            s.position
                .is_some_and(|p| content.experience.iter().any(|r| r.position == p))
        })
        .filter(|s| !cited.contains(s.id.as_str()))
        .collect()
}

/// What the CV leaves out.
pub fn left_out(content: &Content, frame: &Frame) -> LeftOut {
    let roles = frame
        .positions
        .iter()
        .filter(|p| !content.experience.iter().any(|r| r.position == p.index))
        .map(|p| LeftOutRole {
            position: to_u32(p.index),
            title: p.title.clone(),
            company: p.company.clone(),
            start_date: p.start_date.clone(),
            end_date: p.end_date.clone(),
            lines: to_u32(
                frame
                    .of_position(p.index)
                    .filter(|s| {
                        matches!(
                            s.kind,
                            SourceKind::Highlight | SourceKind::Fact | SourceKind::PositionSummary
                        )
                    })
                    .count(),
            ),
        })
        .collect();
    let lines = unused_lines(content, frame)
        .into_iter()
        .filter_map(|s| {
            let position = frame.position(s.position?)?;
            Some(LeftOutLine {
                source: s.id.clone(),
                position: to_u32(position.index),
                title: position.title.clone(),
                company: position.company.clone(),
                text: s.text.clone(),
            })
        })
        .collect();
    LeftOut { roles, lines }
}

/// Words of `text`, lower case: letters and digits, and the `+ # .` a tool's
/// name may hold.
fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#')))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Words that say nothing about what a job wants.
const FILLER: &[&str] = &[
    "and",
    "the",
    "for",
    "with",
    "years",
    "year",
    "experience",
    "production",
    "strong",
    "knowledge",
    "skills",
    "working",
    "work",
    "good",
    "own",
    "you",
    "our",
    "are",
    "your",
];

/// The terms of a keyword that must be found: its words that say something.
fn terms(label: &str) -> Vec<String> {
    let all: Vec<String> = label
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#')))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let say: Vec<String> = all
        .iter()
        .filter(|w| w.chars().count() >= 3 && !FILLER.contains(&w.as_str()) || tech::is_name(w))
        .cloned()
        .collect();
    if say.is_empty() { all } else { say }
}

/// Whether most of the terms of `label` are among `words`.
fn found(label: &str, words: &HashSet<String>) -> bool {
    let terms = terms(label);
    if terms.is_empty() {
        return false;
    }
    let hits = terms
        .iter()
        .filter(|t| {
            words.contains(t.as_str())
                || words
                    .iter()
                    .any(|w| tech::is_name(t) && tech::same_skill(w, t))
        })
        .count();
    hits * 2 >= terms.len()
}

/// The job's keywords: what it wants and the technologies the posting
/// names, each on the CV, in the profile only, or not in the profile. No
/// model: the words of the CV and of the profile.
pub fn keywords(
    wants: &[BriefWant],
    title: &str,
    posting: &str,
    content: &Content,
    frame: &Frame,
) -> Keywords {
    let mut on_cv_text: Vec<&str> = content.lines().map(|(l, _)| l.text.as_str()).collect();
    on_cv_text.extend(content.skills.iter().map(|s| s.name.as_str()));
    let on_cv = words(&on_cv_text.join("\n"));
    let profile_words = words(
        &frame
            .sources
            .iter()
            .filter(|s| s.kind != SourceKind::Company)
            .map(|s| s.text.as_str())
            .chain(frame.skills.iter().map(String::as_str))
            .chain(frame.positions.iter().filter_map(|p| p.title.as_deref()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let state = |label: &str| {
        if found(label, &on_cv) {
            KeywordState::OnCv
        } else if found(label, &profile_words) {
            KeywordState::InProfile
        } else {
            KeywordState::NotInProfile
        }
    };
    let mut items: Vec<Keyword> = wants
        .iter()
        .take(6)
        .map(|w| Keyword {
            label: w.label.clone(),
            kind: KeywordKind::Want,
            level: Some(w.level.clone()),
            state: state(&w.label),
        })
        .collect();
    let mut seen: Vec<String> = Vec::new();
    for token in format!("{title}\n{posting}").split(|c: char| {
        c.is_whitespace() || matches!(c, ',' | ';' | '/' | '(' | ')' | '"' | ':' | '|')
    }) {
        if !tech::is_name(token) {
            continue;
        }
        let canonical = tech::canonical(token);
        let label = token
            .trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#' | '.')))
            .trim_end_matches('.')
            .to_owned();
        let said_by_a_want = items
            .iter()
            .any(|k| terms(&k.label).iter().any(|t| tech::same_skill(t, &label)));
        if seen.contains(&canonical) || said_by_a_want || items.len() >= 16 {
            continue;
        }
        seen.push(canonical);
        let state = state(&label);
        items.push(Keyword {
            label,
            kind: KeywordKind::Tech,
            level: None,
            state,
        });
    }
    let on = items
        .iter()
        .filter(|k| k.state == KeywordState::OnCv)
        .count();
    Keywords {
        total: to_u32(items.len()),
        on_cv: to_u32(on),
        items,
    }
}

/// Everything the page shows of a draft.
pub struct DraftParts<'a> {
    pub id: Uuid,
    pub n: u32,
    pub status: DraftStatus,
    pub version: Option<u32>,
    pub language: &'a str,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub content: &'a Content,
    pub frame: &'a Frame,
    pub checks: &'a Checks,
}

/// The draft as the page shows it.
pub fn draft_view(
    parts: &DraftParts<'_>,
    wants: &[BriefWant],
    title: &str,
    posting: &str,
) -> DraftView {
    let DraftParts {
        content,
        frame,
        checks,
        ..
    } = parts;
    let mut changes = Changes::default();
    let mut count = |view: &LineView| match view.mark {
        Mark::AsInProfile => changes.as_in_profile += 1,
        Mark::MovedUp => changes.moved_up += 1,
        Mark::Reworded => changes.reworded += 1,
        Mark::Edited => changes.edited += 1,
    };
    let headline = content
        .headline
        .as_ref()
        .map(|l| line_view(l, 0, frame, checks));
    let summary = content
        .summary
        .as_ref()
        .map(|l| line_view(l, 0, frame, checks));
    let experience: Vec<RoleView> = content
        .experience
        .iter()
        .filter_map(|role| {
            let info = frame.position(role.position)?;
            Some(RoleView {
                position: to_u32(role.position),
                title: info.title.clone(),
                company: info.company.clone(),
                location: info.location.clone(),
                start_date: info.start_date.clone(),
                end_date: info.end_date.clone(),
                bullets: role
                    .bullets
                    .iter()
                    .enumerate()
                    .map(|(i, b)| line_view(b, i, frame, checks))
                    .collect(),
            })
        })
        .collect();
    for view in headline
        .iter()
        .chain(summary.iter())
        .chain(experience.iter().flat_map(|r| r.bullets.iter()))
    {
        count(view);
    }
    let left_out = left_out(content, frame);
    changes.left_out_roles = to_u32(left_out.roles.len());
    changes.left_out_lines = to_u32(left_out.lines.len());
    let flagged_ids = flagged_in_order(content, checks);
    DraftView {
        n: parts.n,
        status: parts.status,
        version: parts.version,
        language: parts.language.to_owned(),
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
        headline,
        summary,
        experience,
        skills: content
            .skills
            .iter()
            .map(|s| SkillView {
                id: s.id.clone(),
                name: s.name.clone(),
                flags: checks.get(&s.id).cloned().unwrap_or_default(),
            })
            .collect(),
        education: content
            .education
            .iter()
            .map(|e| EducationView {
                institution: e.institution.clone(),
                area: e.area.clone(),
                study_type: e.study_type.clone(),
                start_date: e.start_date.clone(),
                end_date: e.end_date.clone(),
            })
            .collect(),
        languages: content
            .languages
            .iter()
            .map(|l| LanguageView {
                language: l.language.clone(),
                fluency: l.fluency.clone(),
            })
            .collect(),
        left_out,
        flagged: to_u32(flagged_ids.len()),
        first_flagged: flagged_ids.first().cloned(),
        can_approve: parts.status == DraftStatus::Draft && flagged_ids.is_empty(),
        keywords: keywords(wants, title, posting, content, frame),
        changes,
    }
}
