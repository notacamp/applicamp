//! HN "Who is hiring", through Algolia's HN search API: the latest monthly
//! thread by `whoishiring`, and its top-level comments, one job each. They
//! have no structure: the model reads each one's company, role and place
//! when it scores it, and the ones it cannot read are skipped.

use chrono::DateTime;
use serde::{Deserialize, Deserializer};

use crate::job::{Job, Source, html_to_text, one_line};

/// Comment pages read per search (a month's thread has a few hundred
/// comments: one page holds them all).
pub const PAGES: u32 = 2;
/// Comments per page (Algolia's most).
pub const PER_PAGE: u32 = 1_000;

/// `search_by_date?tags=story,author_whoishiring`.
#[derive(Debug, Deserialize)]
pub struct Stories {
    #[serde(default)]
    pub hits: Vec<Story>,
}

/// A thread.
#[derive(Debug, Clone, Deserialize)]
pub struct Story {
    /// Its id.
    #[serde(rename = "objectID", deserialize_with = "id")]
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    /// Seconds since 1970.
    #[serde(default)]
    pub created_at_i: Option<i64>,
}

/// `search_by_date?tags=comment,story_<id>`.
#[derive(Debug, Deserialize)]
pub struct Comments {
    #[serde(default)]
    pub hits: Vec<Comment>,
    /// This page, from 0.
    #[serde(default)]
    pub page: u32,
    /// Pages there are.
    #[serde(default, rename = "nbPages")]
    pub nb_pages: u32,
}

/// A comment.
#[derive(Debug, Deserialize)]
pub struct Comment {
    /// Its id.
    #[serde(rename = "objectID", deserialize_with = "id")]
    pub id: u64,
    /// HTML.
    #[serde(default)]
    pub comment_text: Option<String>,
    /// What it answers.
    #[serde(default)]
    pub parent_id: Option<u64>,
    /// Seconds since 1970.
    #[serde(default)]
    pub created_at_i: Option<i64>,
}

/// An id, as a number or as text.
fn id<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Text(String),
    }
    match Raw::deserialize(d)? {
        Raw::Number(n) => Ok(n),
        Raw::Text(text) => text
            .parse()
            .map_err(|_| serde::de::Error::custom("not an id")),
    }
}

/// The newest "Ask HN: Who is hiring?" thread among `stories`.
pub fn latest_hiring(stories: &Stories) -> Option<Story> {
    stories
        .hits
        .iter()
        .filter(|s| {
            s.title
                .as_deref()
                .is_some_and(|t| t.to_ascii_lowercase().starts_with("ask hn: who is hiring"))
        })
        .max_by_key(|s| s.created_at_i.unwrap_or(0))
        .cloned()
}

/// The jobs of `comments`: the top-level ones of `story`, as text, titled
/// by their first line until the model reads them.
pub fn jobs(comments: Comments, story: &Story) -> Vec<Job> {
    comments
        .hits
        .into_iter()
        .filter(|c| c.parent_id == Some(story.id))
        .filter_map(|c| {
            let text = html_to_text(c.comment_text.as_deref()?);
            let first_line: String = text.lines().next()?.chars().take(160).collect();
            Some(Job {
                source: Source::Hn,
                source_ref: c.id.to_string(),
                title: one_line(Some(&first_line))?,
                company: String::new(),
                location: None,
                work_mode: None,
                board_remote: None,
                url: format!("https://news.ycombinator.com/item?id={}", c.id),
                posted_at: c
                    .created_at_i
                    .and_then(|secs| DateTime::from_timestamp(secs, 0)),
                salary_text: None,
                description: text,
            })
        })
        .collect()
}
