//! RSS 2.0 and Atom feeds: each `<item>` or `<entry>` as a map of its child
//! elements' text, read through the board's field mapping (element names).
//!
//! A small reader for a flat format, not a general XML parser: an item's
//! direct children by name (CDATA unwrapped, characters decoded), Atom's
//! `<link href=…/>`, and the encodings feeds use (UTF-8, and ISO-8859-1 or
//! windows-1252 read as Latin-1).

use std::collections::HashMap;

use crate::job::decode_entities;

use super::mapping::{Item, split_list};

/// Items read per feed, at most.
const MAX_ITEMS: usize = 500;

/// An item: its children's text by element name (the first of each).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedItem(pub HashMap<String, String>);

impl Item for FeedItem {
    fn raw(&self, field: &str) -> Option<String> {
        self.0.get(field).filter(|v| !v.trim().is_empty()).cloned()
    }

    fn list(&self, field: &str) -> Vec<String> {
        self.0.get(field).map(|v| split_list(v)).unwrap_or_default()
    }
}

/// `body` as text, in the encoding its XML declaration names (Latin-1 for
/// ISO-8859-1 and windows-1252), else UTF-8.
pub fn decode(body: &[u8]) -> String {
    let head = String::from_utf8_lossy(&body[..body.len().min(200)]).to_ascii_lowercase();
    let latin1 = head.starts_with("<?xml")
        && ["iso-8859-1", "latin1", "latin-1", "windows-1252", "cp1252"]
            .iter()
            .any(|name| head.contains(name));
    if latin1 {
        body.iter().map(|b| char::from(*b)).collect()
    } else {
        String::from_utf8_lossy(body).into_owned()
    }
}

/// The text of an element's content: CDATA as written, otherwise with its
/// characters decoded.
fn content(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(inner) = raw
        .strip_prefix("<![CDATA[")
        .and_then(|r| r.strip_suffix("]]>"))
    {
        return inner.to_owned();
    }
    decode_entities(raw)
}

/// The value of attribute `name` in a tag's `attrs`.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let at = attrs.find(&format!("{name}="))?;
    let rest = &attrs[at + name.len() + 1..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    Some(decode_entities(&rest[..end]))
}

/// The direct children of an item's inside.
fn children(inside: &str) -> HashMap<String, String> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut at = 0;
    while let Some(open) = inside[at..].find('<').map(|i| i + at) {
        let rest = &inside[open + 1..];
        if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
            at = open + 1;
            continue;
        }
        let Some(end_of_tag) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end_of_tag];
        let name: String = tag
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
            .collect();
        if name.is_empty() {
            at = open + 1;
            continue;
        }
        let attrs = &tag[name.len()..];
        let after_tag = open + 1 + end_of_tag + 1;
        if tag.ends_with('/') {
            // `<link href="…"/>` (Atom).
            if name == "link"
                && let Some(href) = attribute(attrs, "href")
            {
                let rel = attribute(attrs, "rel").unwrap_or_default();
                if rel.is_empty() || rel == "alternate" {
                    fields.entry("link".to_owned()).or_insert(href);
                }
            }
            at = after_tag;
            continue;
        }
        let close = format!("</{name}>");
        let Some(close_at) = inside[after_tag..].find(&close).map(|i| i + after_tag) else {
            at = after_tag;
            continue;
        };
        let text = content(&inside[after_tag..close_at]);
        fields.entry(name).or_insert(text);
        at = close_at + close.len();
    }
    fields
}

/// The items of a feed (RSS `<item>` or Atom `<entry>`), at most 500.
pub fn items(body: &[u8]) -> Vec<FeedItem> {
    let text = decode(body);
    let mut items = Vec::new();
    for (open, close) in [("<item", "</item>"), ("<entry", "</entry>")] {
        let mut at = 0;
        while let Some(start) = text[at..].find(open).map(|i| i + at) {
            let after = &text[start + open.len()..];
            // `<item>` or `<item …>`, never `<itemFoo>`.
            if !after.starts_with('>') && !after.starts_with(char::is_whitespace) {
                at = start + open.len();
                continue;
            }
            let Some(tag_end) = after.find('>') else {
                break;
            };
            let inside_start = start + open.len() + tag_end + 1;
            let Some(end) = text[inside_start..].find(close).map(|i| i + inside_start) else {
                break;
            };
            items.push(FeedItem(children(&text[inside_start..end])));
            if items.len() >= MAX_ITEMS {
                return items;
            }
            at = end + close.len();
        }
        if !items.is_empty() {
            break;
        }
    }
    items
}

/// The feeds a page links to (`<link rel="alternate" type="application/rss+xml" href=…>`).
pub fn feed_links(html: &str) -> Vec<String> {
    let mut links = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<link").map(|i| i + at) {
        let Some(end) = lower[start..].find('>').map(|i| i + start) else {
            break;
        };
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        if tag_lower.contains("alternate")
            && (tag_lower.contains("application/rss+xml")
                || tag_lower.contains("application/atom+xml"))
            && let Some(href) = attribute(tag, "href")
        {
            links.push(href);
        }
        at = end;
        if links.len() >= 5 {
            break;
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Example jobs</title>
<item>
  <title>Example Co: Senior Backend Engineer</title>
  <region>Anywhere in the World</region>
  <link>https://board.example/jobs/1</link>
  <guid>https://board.example/jobs/1</guid>
  <pubDate>Mon, 26 Oct 2026 08:00:12 +0000</pubDate>
  <description><![CDATA[<p>Invented text &amp; more.</p>]]></description>
</item>
<item><title>Other &amp; Co: Staff Engineer</title><link>https://board.example/jobs/2</link></item>
</channel></rss>"#;

    #[test]
    fn rss_items_are_read_by_element() {
        let items = items(RSS.as_bytes());
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].raw("title").as_deref(),
            Some("Example Co: Senior Backend Engineer")
        );
        assert_eq!(
            items[0].raw("region").as_deref(),
            Some("Anywhere in the World")
        );
        assert_eq!(
            items[0].raw("description").as_deref(),
            Some("<p>Invented text &amp; more.</p>"),
            "CDATA as written"
        );
        assert_eq!(
            items[1].raw("title").as_deref(),
            Some("Other & Co: Staff Engineer")
        );
    }

    #[test]
    fn atom_entries_and_latin1_feeds() {
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Role</title>
            <link rel="alternate" href="https://board.example/a"/><updated>2026-10-26T08:00:12Z</updated></entry></feed>"#;
        let items = items(atom.as_bytes());
        assert_eq!(
            items[0].raw("link").as_deref(),
            Some("https://board.example/a")
        );
        let mut latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><rss><channel><item><title>Programador S\xe9nior</title><link>https://x.example/1</link></item></channel></rss>".to_vec();
        latin1.extend_from_slice(b"\n");
        let items = super::items(&latin1);
        assert_eq!(items[0].raw("title").as_deref(), Some("Programador Sénior"));
    }

    #[test]
    fn pages_name_their_feeds() {
        let html = r#"<html lang="en"><head><link rel="alternate" type="application/rss+xml" href="https://board.example/feed.xml"><link rel="stylesheet" href="/x.css"></head></html>"#;
        assert_eq!(feed_links(html), ["https://board.example/feed.xml"]);
    }
}
