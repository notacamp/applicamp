//! A board's items as jobs, through its config's field mapping: one code
//! path for every JSON API, feed and JSON-LD posting.
//!
//! Every value comes from the web and is untrusted: text is trimmed and
//! cut, HTML becomes plain text, links must be http(s), times must be
//! plausible, and an item without a title or a link is skipped.

use chrono::{DateTime, Datelike as _, NaiveDate, NaiveDateTime, Utc};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

use crate::{
    job::{BoardJob, Job, Source, SourceLink, WorkMode, html_to_text, one_line},
    job_boards::{BoardConfig, BoardRow},
    requirements::Salary,
};

/// Longest title, company, place or reference kept, in characters.
const SHORT: usize = 300;
/// Regions kept per job, and the longest.
const REGIONS: (usize, usize) = (30, 80);

/// The board a reading credits.
#[derive(Debug, Clone)]
pub struct BoardRef {
    pub id: Uuid,
    pub slug: String,
    /// How to credit it.
    pub name: String,
    /// Its terms ask to drop expired jobs: their expiry is kept.
    pub drop_expired: bool,
}

impl BoardRef {
    /// The board of `row`.
    pub fn of(row: &BoardRow) -> Self {
        Self {
            id: row.id,
            slug: row.slug.clone(),
            name: row.attribution(),
            drop_expired: row.terms().drop_expired,
        }
    }
}

/// An item of a board's answer.
pub trait Item {
    /// The value at `field` as text (a number as written); `None` when
    /// missing, null or empty.
    fn raw(&self, field: &str) -> Option<String>;
    /// The values at `field`: a list, or text separated by commas.
    fn list(&self, field: &str) -> Vec<String>;
}

impl Item for Value {
    fn raw(&self, field: &str) -> Option<String> {
        let value = self.pointer(field)?;
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return None,
        };
        (!text.trim().is_empty()).then_some(text)
    }

    fn list(&self, field: &str) -> Vec<String> {
        match self.pointer(field) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(o) => o.get("name").and_then(Value::as_str).map(str::to_owned),
                    _ => None,
                })
                .collect(),
            Some(Value::String(text)) => split_list(text),
            Some(Value::Object(o)) => o
                .get("name")
                .and_then(Value::as_str)
                .map(|s| vec![s.to_owned()])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

/// `text` as a list: `['a', 'b']` (a list written as text) or `a, b`.
pub fn split_list(text: &str) -> Vec<String> {
    let text = text.trim();
    let inner = text
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .unwrap_or(text);
    inner
        .split([',', ';', '|'])
        .map(|part| part.trim().trim_matches(['\'', '"']).trim())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A time a board writes: seconds (or milliseconds) since 1970, RFC 3339,
/// RFC 2822, `YYYY-MM-DDTHH:MM:SS` or `YYYY-MM-DD HH:MM:SS` (UTC), or a
/// date; `None` for anything else, or a time before 2000 or after 2100.
pub fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    let at = if let Ok(number) = raw.parse::<f64>() {
        // Whole seconds or milliseconds since 1970.
        #[allow(clippy::cast_possible_truncation)]
        let secs = (if number > 1e11 {
            number / 1000.0
        } else {
            number
        }) as i64;
        DateTime::from_timestamp(secs, 0)?
    } else if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        at.with_timezone(&Utc)
    } else if let Ok(at) = DateTime::parse_from_rfc2822(raw) {
        at.with_timezone(&Utc)
    } else if let Some(at) = ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
    {
        at.and_utc()
    } else {
        NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d")
            .ok()?
            .and_hms_opt(0, 0, 0)?
            .and_utc()
    };
    (2000..=2100).contains(&at.year()).then_some(at)
}

/// A number a board writes, above zero (`"0"` is none).
fn amount(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '_'))
        .collect();
    cleaned
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0 && *n < 1e9)
}

/// `raw` as an http(s) link.
pub fn link(raw: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .then(|| url.to_string().chars().take(2_000).collect())
}

/// `raw`, a relative link or an id (`517003`, `Example-Co-Engineer`), read
/// against `base`; only a link on `base`'s own host.
pub fn relative_link(base: &Url, raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || Url::parse(raw).is_ok() {
        // Empty, or absolute (and not the web: `link` refused it).
        return None;
    }
    let url = base.join(raw).ok()?;
    if url.host_str() != base.host_str() {
        return None;
    }
    link(url.as_str())
}

fn short(text: Option<String>) -> Option<String> {
    one_line(text.as_deref()).map(|t| t.chars().take(SHORT).collect())
}

/// Pay, as the model and the person read it: "USD 90000–112000 per year".
fn pay_text(pay: &Salary) -> String {
    let number = |n: f64| format!("{n:.0}");
    let range = match (pay.min, pay.max) {
        (Some(min), Some(max)) if (max - min).abs() > f64::EPSILON => {
            format!("{}–{}", number(min), number(max))
        }
        (Some(one), _) | (None, Some(one)) => number(one),
        (None, None) => String::new(),
    };
    let currency = pay.currency.as_deref().unwrap_or("");
    let period = pay
        .period
        .as_deref()
        .map(|p| format!(" per {p}"))
        .unwrap_or_default();
    format!("{currency} {range}{period}").trim().to_owned()
}

/// `item` as a job of `board`, read with `config`; `None` when it has no
/// title or no link, or `config`'s filter leaves it out.
pub fn job_of(item: &impl Item, config: &BoardConfig, board: &BoardRef) -> Option<Job> {
    let f = &config.fields;
    let get = |field: &Option<String>| field.as_deref().and_then(|p| item.raw(p));
    let link_of = |raw: &str| {
        link(raw).or_else(|| {
            config
                .link_base
                .as_ref()
                .and_then(|base| relative_link(base, raw))
        })
    };
    let url = link_of(&get(&f.url)?)?;
    let mut title = short(get(&f.title))?;
    let mut company = short(get(&f.company)).unwrap_or_default();
    if let Some(split) = &config.title_split
        && company.is_empty()
        && !split.separator.is_empty()
        && let Some((left, right)) = title.split_once(split.separator.as_str())
    {
        let (left, right) = (left.trim().to_owned(), right.trim().to_owned());
        if !left.is_empty() && !right.is_empty() {
            (company, title) = if split.company_first {
                (left, right)
            } else {
                (right, left)
            };
        }
    }
    let source_ref = short(get(&f.reference)).unwrap_or_else(|| url.clone());
    let regions: Vec<String> = f
        .regions
        .as_deref()
        .map(|p| item.list(p))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| one_line(Some(&r)).map(|r| r.chars().take(REGIONS.1).collect()))
        .take(REGIONS.0)
        .collect();
    let location = short(get(&f.location)).or_else(|| {
        (!regions.is_empty()).then(|| regions.join(", ").chars().take(SHORT).collect())
    });
    let pay = {
        let min = get(&f.salary_min).as_deref().and_then(amount);
        let max = get(&f.salary_max).as_deref().and_then(amount);
        let currency = get(&f.salary_currency)
            .or_else(|| config.salary_currency.clone())
            .map(|c| c.trim().to_ascii_uppercase())
            .filter(|c| c.len() == 3 && c.chars().all(|ch| ch.is_ascii_uppercase()));
        let period = get(&f.salary_period)
            .or_else(|| config.salary_period.clone())
            .map(|p| p.trim().to_ascii_lowercase().chars().take(20).collect());
        (min.is_some() || max.is_some()).then_some(Salary {
            min,
            max,
            currency,
            period,
        })
    };
    let salary_text = short(get(&f.salary_text)).or_else(|| pay.as_ref().map(pay_text));
    let expires_at = if board.drop_expired {
        get(&f.expires_at).as_deref().and_then(parse_time)
    } else {
        None
    };
    Some(Job {
        source: Source::Board,
        source_ref,
        title,
        company,
        location,
        work_mode: if config.remote_only {
            Some(WorkMode::Remote)
        } else {
            get(&f.work_mode).as_deref().and_then(WorkMode::parse)
        },
        // A board's "remote" is never a requirement's evidence: the model
        // reads whether it is open to where the person lives.
        board_remote: None,
        posted_at: get(&f.posted_at).as_deref().and_then(parse_time),
        salary_text,
        description: get(&f.description)
            .as_deref()
            .map(html_to_text)
            .unwrap_or_default(),
        board: Some(BoardJob {
            slug: board.slug.clone(),
            links: vec![SourceLink {
                board: board.slug.clone(),
                name: board.name.clone(),
                url: url.clone(),
            }],
            apply_url: get(&f.apply_url).as_deref().and_then(link_of),
            regions,
            expires_at,
            pay,
        }),
        url,
    })
}

/// Whether `item` passes `config`'s filter (none: every item).
pub fn kept(item: &Value, config: &BoardConfig) -> bool {
    config
        .filter
        .as_ref()
        .is_none_or(|filter| item.pointer(&filter.pointer) == Some(&filter.equals))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::job_boards::Fields;

    #[test]
    fn times_are_read_in_every_shape_a_board_writes() {
        let expect = DateTime::parse_from_rfc3339("2026-10-26T08:00:12Z").unwrap();
        for raw in [
            "1793001612",
            "1793001612000",
            "2026-10-26T08:00:12+00:00",
            "Mon, 26 Oct 2026 08:00:12 +0000",
            "2026-10-26T08:00:12",
            "2026-10-26 08:00:12",
        ] {
            assert_eq!(parse_time(raw), Some(expect.with_timezone(&Utc)), "{raw}");
        }
        assert!(parse_time("2026-10-26").is_some());
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(parse_time("12"), None, "1970 is not a posting date");
    }

    #[test]
    fn lists_are_read_from_lists_and_text() {
        assert_eq!(
            split_list("Germany,  Portugal,  Spain,  UK"),
            ["Germany", "Portugal", "Spain", "UK"]
        );
        assert_eq!(split_list("['video', 'AI/ML']"), ["video", "AI/ML"]);
        let item = json!({ "a": ["United States", 3, { "name": "Canada" }] });
        assert_eq!(item.list("/a"), ["United States", "Canada"]);
    }

    #[test]
    fn an_item_becomes_a_job_crediting_its_board() {
        let config = BoardConfig {
            remote_only: true,
            salary_period: Some("year".into()),
            fields: Fields {
                reference: Some("/id".into()),
                title: Some("/title".into()),
                company: Some("/company".into()),
                url: Some("/url".into()),
                apply_url: Some("/apply".into()),
                regions: Some("/geo".into()),
                description: Some("/html".into()),
                salary_min: Some("/min".into()),
                salary_max: Some("/max".into()),
                salary_currency: Some("/cur".into()),
                expires_at: Some("/expires".into()),
                ..Fields::default()
            },
            ..BoardConfig::default()
        };
        let board = BoardRef {
            id: Uuid::nil(),
            slug: "himalayas".into(),
            name: "Himalayas".into(),
            drop_expired: true,
        };
        let item = json!({
            "id": 7, "title": " Staff   Engineer ", "company": "Example Co",
            "url": "https://board.example/jobs/7", "apply": "https://example.com/apply?utm_source=api",
            "geo": "Germany, Portugal", "html": "<p>We build <b>things</b>.</p><script>x()</script>",
            "min": "90000", "max": 0, "cur": "eur", "expires": 1798286983,
        });
        let job = job_of(&item, &config, &board).unwrap();
        assert_eq!(job.title, "Staff Engineer");
        assert_eq!(job.source_ref, "7");
        assert_eq!(job.source_name(), "himalayas");
        assert_eq!(job.location.as_deref(), Some("Germany, Portugal"));
        assert_eq!(job.work_mode, Some(WorkMode::Remote));
        assert_eq!(job.description, "We build things .");
        assert_eq!(job.salary_text.as_deref(), Some("EUR 90000 per year"));
        let on = job.board.unwrap();
        assert_eq!(
            on.apply_url.as_deref(),
            Some("https://example.com/apply?utm_source=api")
        );
        assert_eq!(on.links[0].name, "Himalayas");
        assert_eq!(on.regions, ["Germany", "Portugal"]);
        assert!(on.expires_at.is_some());
        assert_eq!(on.pay.unwrap().max, None, "0 is no salary");
        // No link, or a link that is not the web: no job.
        let bad = json!({ "title": "X", "url": "javascript:alert(1)" });
        assert!(job_of(&bad, &config, &board).is_none());
    }

    #[test]
    fn a_relative_link_is_read_against_the_link_base() {
        let config = BoardConfig {
            link_base: Some("https://board.example/jobs/".parse().unwrap()),
            salary_currency: Some("CHF".into()),
            salary_period: Some("year".into()),
            fields: Fields {
                reference: Some("/_id".into()),
                title: Some("/name".into()),
                company: Some("/company".into()),
                url: Some("/jobUrl".into()),
                salary_min: Some("/annualSalaryFrom".into()),
                salary_max: Some("/annualSalaryTo".into()),
                work_mode: Some("/workplace".into()),
                ..Fields::default()
            },
            ..BoardConfig::default()
        };
        let board = BoardRef {
            id: Uuid::nil(),
            slug: "swissdevjobs".into(),
            name: "SwissDevJobs".into(),
            drop_expired: false,
        };
        let item = json!({
            "_id": "a1", "name": "Backend Engineer", "company": "Example AG",
            "jobUrl": "Example-AG-Backend-Engineer", "annualSalaryFrom": 100000,
            "annualSalaryTo": 120000, "workplace": "hybrid",
        });
        let job = job_of(&item, &config, &board).unwrap();
        assert_eq!(
            job.url,
            "https://board.example/jobs/Example-AG-Backend-Engineer"
        );
        assert_eq!(job.work_mode, Some(WorkMode::Hybrid));
        assert_eq!(
            job.salary_text.as_deref(),
            Some("CHF 100000–120000 per year")
        );
        // A relative link never leads to another host or scheme.
        for raw in ["//evil.example/x", "javascript:alert(1)"] {
            let item = json!({ "name": "X", "jobUrl": raw });
            assert!(job_of(&item, &config, &board).is_none(), "{raw}");
        }
    }
}
