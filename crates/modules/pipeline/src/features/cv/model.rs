//! What a tailored CV is made of, as it is stored: the sources the model was
//! shown (the frame), and the draft built from them (the content).
//!
//! Every line of the content cites source ids; a source is a piece of the
//! person's own profile (or a fact read on the company's site) with the id
//! the model must cite. Contact details are in the content (the server fills
//! them) and never in the frame, so the part the model is shown cannot hold
//! them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ports::{CvContact, CvProfile};

/// The profile's headline.
pub const HEADLINE: &str = "hl";
/// The profile's summary.
pub const SUMMARY: &str = "sum";
/// The skills list.
pub const SKILLS: &str = "sk";
/// Education.
pub const EDUCATION: &str = "ed";
/// Languages.
pub const LANGUAGES: &str = "lang";

/// What a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// The profile's headline (`hl`).
    Headline,
    /// The profile's summary (`sum`).
    Summary,
    /// A position's summary (`w3.s`).
    PositionSummary,
    /// A position's highlight (`w3.h1`).
    Highlight,
    /// A fact the person kept on a position (`w3.f<id>`).
    Fact,
    /// A fact read on the company's own site (`c<id>`).
    Company,
    /// The skills list (`sk`).
    Skills,
    /// Education (`ed`).
    Education,
    /// Languages (`lang`).
    Languages,
}

/// A piece of the person's profile (or a company fact), with its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    /// Its words, as the profile (or the fact) has them.
    pub text: String,
    /// The position it belongs to (its place in the profile's `work`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
    /// Its place among its position's highlights and facts, from 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<usize>,
    /// For a company fact, the page it was read on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A position of the profile, as the CV heads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionInfo {
    pub index: usize,
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

impl PositionInfo {
    /// The words of its head, which a line about it may repeat.
    fn head(&self) -> String {
        [
            &self.title,
            &self.company,
            &self.location,
            &self.start_date,
            &self.end_date,
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
    }
}

/// Everything a line of the CV may cite.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub sources: Vec<Source>,
    pub positions: Vec<PositionInfo>,
    /// The person's skills, by name.
    pub skills: Vec<String>,
    /// The company the job is at.
    pub company: String,
    /// The most recent position.
    pub latest: Option<usize>,
}

/// A fact read on the company's site, as a source.
pub struct CompanyFact {
    pub id: String,
    pub text: String,
    pub url: String,
}

/// `id`, lower case, letters and digits only, at most 8: the part of a fact's
/// id a source id carries.
pub fn short(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_lowercase()
}

/// `s` trimmed; none when it is blank.
fn nonblank(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|t| !t.is_empty())
}

impl Frame {
    /// The sources of `profile` and the company's `facts`.
    pub fn build(profile: &CvProfile, company: &str, facts: &[CompanyFact]) -> Self {
        let mut sources = Vec::new();
        let mut add = |id: String, kind, text: String, position, order, url| {
            sources.push(Source {
                id,
                kind,
                text,
                position,
                order,
                url,
            });
        };
        if let Some(text) = nonblank(&profile.headline) {
            add(
                HEADLINE.into(),
                SourceKind::Headline,
                text.to_owned(),
                None,
                None,
                None,
            );
        }
        if let Some(text) = nonblank(&profile.summary) {
            add(
                SUMMARY.into(),
                SourceKind::Summary,
                text.to_owned(),
                None,
                None,
                None,
            );
        }
        for position in &profile.positions {
            let at = position.index;
            if let Some(text) = nonblank(&position.summary) {
                add(
                    format!("w{at}.s"),
                    SourceKind::PositionSummary,
                    text.to_owned(),
                    Some(at),
                    None,
                    None,
                );
            }
            let mut order = 0;
            for (k, highlight) in position.highlights.iter().enumerate() {
                let text = highlight.trim();
                if text.is_empty() {
                    continue;
                }
                add(
                    format!("w{at}.h{}", k + 1),
                    SourceKind::Highlight,
                    text.to_owned(),
                    Some(at),
                    Some(order),
                    None,
                );
                order += 1;
            }
            for fact in &position.facts {
                let text = fact.text.trim();
                if text.is_empty() {
                    continue;
                }
                add(
                    format!("w{at}.f{}", short(&fact.id)),
                    SourceKind::Fact,
                    text.to_owned(),
                    Some(at),
                    Some(order),
                    None,
                );
                order += 1;
            }
        }
        for fact in facts {
            add(
                format!("c{}", short(&fact.id)),
                SourceKind::Company,
                fact.text.trim().to_owned(),
                None,
                None,
                Some(fact.url.clone()),
            );
        }
        if !profile.skills.is_empty() {
            add(
                SKILLS.into(),
                SourceKind::Skills,
                profile.skills.join(", "),
                None,
                None,
                None,
            );
        }
        let education: Vec<String> = profile.education.iter().map(education_line).collect();
        if !education.is_empty() {
            add(
                EDUCATION.into(),
                SourceKind::Education,
                education.join("; "),
                None,
                None,
                None,
            );
        }
        let languages: Vec<String> = profile.languages.iter().map(language_line).collect();
        if !languages.is_empty() {
            add(
                LANGUAGES.into(),
                SourceKind::Languages,
                languages.join(", "),
                None,
                None,
                None,
            );
        }
        Self {
            sources,
            positions: profile
                .positions
                .iter()
                .map(|p| PositionInfo {
                    index: p.index,
                    title: p.title.clone(),
                    company: p.company.clone(),
                    location: p.location.clone(),
                    start_date: p.start_date.clone(),
                    end_date: p.end_date.clone(),
                })
                .collect(),
            skills: profile.skills.clone(),
            company: company.trim().to_owned(),
            latest: profile.latest,
        }
    }

    /// The source `id`.
    pub fn get(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// The position at `index` of the profile.
    pub fn position(&self, index: usize) -> Option<&PositionInfo> {
        self.positions.iter().find(|p| p.index == index)
    }

    /// The sources of position `index`, in the profile's order.
    pub fn of_position(&self, index: usize) -> impl Iterator<Item = &Source> {
        self.sources
            .iter()
            .filter(move |s| s.position == Some(index))
    }

    /// The words the sources `cited` (unknown ids are ignored) say, with the
    /// head of the positions they belong to and the company's name for a
    /// company fact: what a line citing them may repeat.
    pub fn evidence(&self, cited: &[String]) -> Vec<String> {
        let mut texts = Vec::new();
        let mut heads: Vec<usize> = Vec::new();
        let mut company = false;
        for source in cited.iter().filter_map(|id| self.get(id)) {
            texts.push(source.text.clone());
            if let Some(at) = source.position
                && !heads.contains(&at)
            {
                heads.push(at);
            }
            company |= source.kind == SourceKind::Company;
        }
        for at in heads {
            if let Some(position) = self.position(at) {
                texts.push(position.head());
            }
        }
        if company {
            texts.push(self.company.clone());
        }
        texts
    }
}

fn education_line(e: &crate::ports::CvEducation) -> String {
    [
        e.study_type.as_deref(),
        e.area.as_deref(),
        Some(e.institution.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

fn language_line(l: &crate::ports::CvLanguage) -> String {
    match l.fluency.as_deref() {
        Some(fluency) if !fluency.trim().is_empty() => format!("{} ({})", l.language, fluency),
        _ => l.language.clone(),
    }
}

/// A line of the CV: its words and the sources it says they come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub id: String,
    pub text: String,
    pub sources: Vec<String>,
    /// The person wrote this text.
    #[serde(default)]
    pub edited: bool,
}

/// A position on the CV, with the bullets chosen for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub position: usize,
    pub bullets: Vec<Line>,
}

/// A skill on the CV.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillItem {
    pub id: String,
    pub name: String,
}

/// How to reach the person: the server's, from the profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub name: Option<String>,
    pub email: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub links: Vec<(Option<String>, String)>,
}

impl From<CvContact> for Contact {
    fn from(c: CvContact) -> Self {
        Self {
            name: c.name,
            email: c.email,
            location: c.location,
            url: c.url,
            links: c.links,
        }
    }
}

/// A school on the CV, as the profile has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EducationItem {
    pub institution: String,
    pub area: Option<String>,
    pub study_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// A language on the CV, as the profile has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageItem {
    pub language: String,
    pub fluency: Option<String>,
}

/// The draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub contact: Contact,
    pub headline: Option<Line>,
    pub summary: Option<Line>,
    pub experience: Vec<Role>,
    pub skills: Vec<SkillItem>,
    pub education: Vec<EducationItem>,
    pub languages: Vec<LanguageItem>,
    /// The next number of a line id (`l<n>`).
    pub next_line: u32,
}

impl Content {
    /// A new line id.
    pub fn new_line_id(&mut self) -> String {
        self.next_line += 1;
        format!("l{}", self.next_line)
    }

    /// Every line of the CV (headline, summary, bullets), with whether it is
    /// the summary.
    pub fn lines(&self) -> impl Iterator<Item = (&Line, bool)> {
        self.headline
            .iter()
            .map(|l| (l, false))
            .chain(self.summary.iter().map(|l| (l, true)))
            .chain(
                self.experience
                    .iter()
                    .flat_map(|r| r.bullets.iter().map(|l| (l, false))),
            )
    }

    /// The position a line is about: its role's, else the most recent one.
    pub fn position_of(&self, line: &str, latest: Option<usize>) -> Option<usize> {
        self.experience
            .iter()
            .find(|r| r.bullets.iter().any(|b| b.id == line))
            .map(|r| r.position)
            .or(latest)
    }
}

/// The profile's education and languages, as the CV prints them.
pub fn education_of(profile: &CvProfile) -> Vec<EducationItem> {
    profile
        .education
        .iter()
        .map(|e| EducationItem {
            institution: e.institution.clone(),
            area: e.area.clone(),
            study_type: e.study_type.clone(),
            start_date: e.start_date.clone(),
            end_date: e.end_date.clone(),
        })
        .collect()
}

/// See [`education_of`].
pub fn languages_of(profile: &CvProfile) -> Vec<LanguageItem> {
    profile
        .languages
        .iter()
        .map(|l| LanguageItem {
            language: l.language.clone(),
            fluency: l.fluency.clone(),
        })
        .collect()
}
