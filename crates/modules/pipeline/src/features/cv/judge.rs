//! What the model answered, made into a draft: shaped and capped, never
//! believed. What it says is checked afterwards by [`super::check`].

use serde::Deserialize;

use super::model::{Contact, Content, Frame, Line, Role, SkillItem};
use crate::ports::CvProfile;

/// Longest headline, summary and bullet, in characters.
pub const MAX_HEADLINE: usize = 200;
pub const MAX_SUMMARY: usize = 700;
pub const MAX_BULLET: usize = 400;
/// Bullets for one position, and for the whole CV.
pub const MAX_ROLE_BULLETS: usize = 8;
pub const MAX_BULLETS: usize = 16;
/// Skills shown, and the longest skill name.
pub const MAX_SKILLS: usize = 16;
pub const MAX_SKILL: usize = 80;
/// Sources one line may cite.
const MAX_CITED: usize = 8;

/// A line as the model gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawLine {
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

/// A position as the model gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct RawRole {
    pub position: i64,
    #[serde(default)]
    pub bullets: Vec<RawLine>,
}

/// The model's answer.
#[derive(Debug, Clone, Deserialize)]
pub struct RawCv {
    #[serde(default)]
    pub headline: Option<RawLine>,
    #[serde(default)]
    pub summary: Option<RawLine>,
    #[serde(default)]
    pub experience: Vec<RawRole>,
    #[serde(default)]
    pub skills: Vec<String>,
}

fn tidy(raw: Option<RawLine>, most: usize, content: &mut Content) -> Option<Line> {
    let raw = raw?;
    let text = raw.text.trim();
    if text.is_empty() {
        return None;
    }
    Some(line(content, text, raw.sources, most))
}

fn line(content: &mut Content, text: &str, sources: Vec<String>, most: usize) -> Line {
    let mut cited: Vec<String> = Vec::new();
    for id in sources {
        let id = id.trim().to_owned();
        if !id.is_empty() && !cited.contains(&id) && cited.len() < MAX_CITED {
            cited.push(id);
        }
    }
    Line {
        id: content.new_line_id(),
        text: text.chars().take(most).collect(),
        sources: cited,
        edited: false,
    }
}

/// The draft the model's answer comes to, with `profile`'s contact and
/// education and languages, word for word. `None` when it says nothing.
pub fn judge(raw: RawCv, frame: &Frame, profile: &CvProfile) -> Option<Content> {
    let mut content = Content {
        contact: Contact::from(profile.contact.clone()),
        education: super::model::education_of(profile),
        languages: super::model::languages_of(profile),
        ..Content::default()
    };
    content.headline = tidy(raw.headline, MAX_HEADLINE, &mut content);
    content.summary = tidy(raw.summary, MAX_SUMMARY, &mut content);
    let mut total = 0;
    for role in raw.experience {
        let Some(position) = usize::try_from(role.position)
            .ok()
            .filter(|p| frame.position(*p).is_some())
        else {
            continue;
        };
        if content.experience.iter().any(|r| r.position == position) {
            continue;
        }
        let mut bullets = Vec::new();
        for bullet in role.bullets.into_iter().take(MAX_ROLE_BULLETS) {
            if total >= MAX_BULLETS {
                break;
            }
            if let Some(bullet) = tidy(Some(bullet), MAX_BULLET, &mut content) {
                bullets.push(bullet);
                total += 1;
            }
        }
        content.experience.push(Role { position, bullets });
    }
    let mut skills: Vec<String> = Vec::new();
    for skill in raw.skills {
        let skill = skill.trim();
        if skill.is_empty()
            || skill.chars().count() > MAX_SKILL
            || skills.iter().any(|s| s.eq_ignore_ascii_case(skill))
            || skills.len() >= MAX_SKILLS
        {
            continue;
        }
        skills.push(skill.to_owned());
    }
    for name in skills {
        let id = content.new_line_id();
        content.skills.push(SkillItem { id, name });
    }
    let empty = content.headline.is_none()
        && content.summary.is_none()
        && content.experience.iter().all(|r| r.bullets.is_empty());
    (!empty).then_some(content)
}

/// A line of the CV by the person's hand: the same caps as the model's.
pub fn edited(text: &str, most: usize) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= most).then(|| text.to_owned())
}
