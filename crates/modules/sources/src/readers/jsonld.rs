//! JSON-LD `JobPosting`s found through a sitemap: the third adapter, for a
//! board with neither an API nor a feed, read only once the owner approved
//! it and only on paths its robots rules allow. No seeded board uses it
//! yet.
//!
//! The sitemap's `<loc>`s under the config's `job_path` are the postings
//! (at most `max_jobs` a fetch); each page's `<script
//! type="application/ld+json">` blocks are read for objects of `@type`
//! `JobPosting` (also inside `@graph` and lists), mapped with [`FIELDS`].

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::{
    job::decode_entities,
    job_boards::{BoardConfig, Fields},
};

static LOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<loc>\s*(.*?)\s*</loc>")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});
static LD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<script[^>]*type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script>"#)
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Where schema.org's `JobPosting` keeps each field.
pub fn fields() -> Fields {
    Fields {
        reference: Some("/url".into()),
        title: Some("/title".into()),
        company: Some("/hiringOrganization/name".into()),
        url: Some("/url".into()),
        apply_url: Some("/url".into()),
        location: Some("/jobLocation/address/addressLocality".into()),
        regions: Some("/applicantLocationRequirements".into()),
        description: Some("/description".into()),
        posted_at: Some("/datePosted".into()),
        expires_at: Some("/validThrough".into()),
        salary_min: Some("/baseSalary/value/minValue".into()),
        salary_max: Some("/baseSalary/value/maxValue".into()),
        salary_currency: Some("/baseSalary/currency".into()),
        salary_period: Some("/baseSalary/value/unitText".into()),
        salary_text: None,
    }
}

/// [`fields`] in `config`, unless it maps its own.
pub fn config_with_fields(config: &BoardConfig) -> BoardConfig {
    let mut config = config.clone();
    if config.fields == Fields::default() {
        config.fields = fields();
    }
    config
}

/// The posting pages a sitemap lists under `job_path`, at most `most`.
pub fn sitemap_urls(body: &str, job_path: Option<&str>, most: usize) -> Vec<url::Url> {
    LOC.captures_iter(body)
        .filter_map(|caps| caps.get(1))
        .filter_map(|m| url::Url::parse(&decode_entities(m.as_str())).ok())
        .filter(|url| matches!(url.scheme(), "https" | "http"))
        .filter(|url| job_path.is_none_or(|prefix| url.path().starts_with(prefix)))
        .take(most)
        .collect()
}

fn collect(value: Value, into: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect(item, into);
            }
        }
        Value::Object(mut object) => {
            if let Some(graph) = object.remove("@graph") {
                collect(graph, into);
            }
            let is_posting = match object.get("@type") {
                Some(Value::String(t)) => t == "JobPosting",
                Some(Value::Array(types)) => types.iter().any(|t| t == "JobPosting"),
                _ => false,
            };
            if is_posting {
                into.push(Value::Object(object));
            }
        }
        _ => {}
    }
}

/// The `JobPosting`s of a page, each with its `url` set to `page` when it
/// has none.
pub fn postings(html: &str, page: &url::Url) -> Vec<Value> {
    let mut found = Vec::new();
    for caps in LD.captures_iter(html) {
        let Some(block) = caps.get(1) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(block.as_str().trim()) {
            collect(value, &mut found);
        }
    }
    for posting in &mut found {
        if posting.get("url").and_then(Value::as_str).is_none()
            && let Some(object) = posting.as_object_mut()
        {
            object.insert("url".to_owned(), Value::String(page.to_string()));
        }
        // Remote, as schema.org says it.
        let telecommute = posting
            .get("jobLocationType")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("TELECOMMUTE"));
        if telecommute && let Some(object) = posting.as_object_mut() {
            object.insert("remote".to_owned(), Value::Bool(true));
        }
    }
    found
}

/// Whether `html` holds a `JobPosting`.
pub fn has_postings(html: &str) -> bool {
    LD.captures_iter(html).any(|caps| {
        caps.get(1)
            .is_some_and(|m| m.as_str().contains("JobPosting"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::mapping::{BoardRef, job_of};

    #[test]
    fn sitemaps_list_postings_and_pages_hold_them() {
        let sitemap = "<urlset><url><loc>https://board.example/jobs/1</loc></url>\
                       <url><loc>https://board.example/about</loc></url>\
                       <url><loc>https://board.example/jobs/2</loc></url></urlset>";
        let urls = sitemap_urls(sitemap, Some("/jobs/"), 10);
        assert_eq!(urls.len(), 2);
        let page = r#"<html><script type="application/ld+json">
            {"@context":"https://schema.org","@graph":[{"@type":"Organization","name":"X"},
             {"@type":"JobPosting","title":"Staff Engineer","hiringOrganization":{"name":"Example Co"},
              "datePosted":"2026-10-20","jobLocationType":"TELECOMMUTE",
              "applicantLocationRequirements":[{"@type":"Country","name":"Portugal"}],
              "baseSalary":{"currency":"EUR","value":{"minValue":80000,"maxValue":100000,"unitText":"YEAR"}},
              "description":"<p>Invented.</p>"}]}
            </script></html>"#;
        assert!(has_postings(page));
        let found = postings(page, &urls[0]);
        assert_eq!(found.len(), 1);
        let config = config_with_fields(&BoardConfig::default());
        let board = BoardRef {
            id: uuid::Uuid::nil(),
            slug: "example".into(),
            name: "Example".into(),
            drop_expired: false,
        };
        let job = job_of(&found[0], &config, &board).unwrap();
        assert_eq!(job.company, "Example Co");
        assert_eq!(job.url, "https://board.example/jobs/1");
        let on = job.board.unwrap();
        assert_eq!(on.regions, ["Portugal"]);
        assert_eq!(on.pay.unwrap().period.as_deref(), Some("year"));
    }
}
