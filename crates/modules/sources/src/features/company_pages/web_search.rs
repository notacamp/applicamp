//! The optional weekly web search that finds company pages (#87).
//!
//! Off unless `APPLICAMP_SEARCH__BRAVE_API_KEY_FILE` names a file holding a
//! Brave Search API key. Once a week it asks at most
//! `APPLICAMP_SEARCH__BRAVE_WEEKLY_QUERIES` queries (50), built from the
//! roles every account's goals name and the addresses of the approved hiring
//! systems (`site:jobs.lever.co "backend engineer"`), and never more than
//! `APPLICAMP_SEARCH__BRAVE_MONTHLY_QUERIES` (1000) in a calendar month:
//! Brave bills each query, so each one asked is counted in
//! `sources.web_search_queries`, answered or not.
//!
//! Brave is a search API, not the hiring site: its answer is used for its
//! links only. Each link on a hiring system's public address becomes a
//! candidate company page ([`crate::company_pages::register`]); no title,
//! snippet or other text of the answer is kept, and nothing is read from the
//! company's page until the daily check does it.

use std::{collections::BTreeSet, time::Duration};

use applicamp_kernel::{Error, JobContext, JobError, Result, jobs};
use chrono::{DateTime, Datelike, TimeDelta, Utc, Weekday};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    SourcesDeps,
    hiring_systems::{self, CompanyLink, System},
    job_boards::BoardRow,
    readers::USER_AGENT,
};

/// Links read from one query's answer, at most.
const RESULTS: usize = 20;
/// The longest role a query is built from.
const MAX_ROLE: usize = 80;

/// The first moment of `now`'s calendar month, UTC.
pub fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive()
        .with_day(1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map_or(now, |at| at.and_utc())
}

/// The next Monday at 05:30 UTC after `now`.
pub fn next_run(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut at = now
        .date_naive()
        .and_hms_opt(5, 30, 0)
        .map_or(now, |t| t.and_utc());
    while at <= now || at.weekday() != Weekday::Mon {
        at += TimeDelta::days(1);
    }
    at
}

/// The weekly web search.
#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearch {}

impl applicamp_kernel::Job for WebSearch {
    const NAME: &'static str = "sources.web_search_pages";
    const QUEUE: &'static str = "boards";
    // Never retried: next week tries again, and Brave bills each query.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        schedule_next(&ctx.db, deps).await?;
        run(deps, &ctx.db).await.map(|_| ()).map_err(JobError::from)
    }
}

/// Make sure the next weekly search is scheduled, when a key is set.
/// Whether one was added.
pub async fn schedule(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.search.brave() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_unique_at(&mut tx, &WebSearch {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

async fn schedule_next(db: &PgPool, deps: &SourcesDeps) -> Result<bool> {
    if !deps.config.search.brave() {
        return Ok(false);
    }
    let mut tx = db.begin().await?;
    let added = jobs::enqueue_next_at(&mut tx, &WebSearch {}, next_run(deps.clock.now())).await?;
    tx.commit().await?;
    Ok(added)
}

/// A role as a query may carry it: a few plain words, no quote.
fn plain_role(role: &str) -> Option<String> {
    let role: String = role
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '/' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let role = role
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (role.chars().count() >= 3 && role.chars().count() <= MAX_ROLE).then_some(role)
}

/// The queries for `roles` on `systems`, every role on every system, in a
/// fixed order.
pub fn queries(roles: &[String], systems: &[&System]) -> Vec<String> {
    let roles: BTreeSet<String> = roles.iter().filter_map(|r| plain_role(r)).collect();
    let mut found = Vec::new();
    for role in &roles {
        for system in systems {
            found.push(format!("site:{} \"{role}\"", system.search_host));
        }
    }
    found
}

/// The `count` queries of `all` this week asks: a window that moves along
/// the list week after week, so that every query is asked in turn.
pub fn this_weeks(all: &[String], count: usize, now: DateTime<Utc>) -> Vec<String> {
    if all.is_empty() || count == 0 {
        return Vec::new();
    }
    let week = usize::try_from(now.timestamp().max(0) / (7 * 24 * 3600)).unwrap_or(0);
    let start = week.wrapping_mul(count) % all.len();
    all.iter()
        .cycle()
        .skip(start)
        .take(count.min(all.len()))
        .cloned()
        .collect()
}

/// The links of a Brave answer.
pub fn links_of(answer: &serde_json::Value) -> Vec<CompanyLink> {
    let mut links: Vec<CompanyLink> = Vec::new();
    for result in answer
        .pointer("/web/results")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .take(RESULTS)
    {
        let Some(link) = result
            .get("url")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| Url::parse(raw).ok())
            .as_ref()
            .and_then(hiring_systems::company_link)
        else {
            continue;
        };
        if !links.contains(&link) {
            links.push(link);
        }
    }
    links
}

/// What a weekly search did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Queries asked.
    pub queries: usize,
    /// Company pages the links added.
    pub added: usize,
}

/// One weekly search.
pub async fn run(deps: &SourcesDeps, db: &PgPool) -> Result<Summary> {
    let mut summary = Summary::default();
    let Some(file) = deps.config.search.brave_api_key_file.clone() else {
        return Ok(summary);
    };
    let key = match tokio::fs::read_to_string(&file).await {
        Ok(key) => key.trim().to_owned(),
        Err(_) => {
            // The file's name only: never its content, never the key.
            tracing::warn!("the web search key file could not be read");
            return Ok(summary);
        }
    };
    if key.is_empty() {
        tracing::warn!("the web search key file is empty");
        return Ok(summary);
    }
    let now = deps.clock.now();
    let used = u64::from(super::queries_this_month(db, now).await?);
    let room = deps.config.search.monthly_queries().saturating_sub(used);
    let count = usize::try_from(deps.config.search.weekly_queries().min(room)).unwrap_or(0);
    if count == 0 {
        tracing::info!("the web search is at its monthly limit");
        return Ok(summary);
    }
    let Some(neighbours) = &deps.neighbours else {
        return Ok(summary);
    };
    let mut roles: Vec<String> = Vec::new();
    for account in neighbours.accounts_with_goals(db).await? {
        if let Some(input) = neighbours.matching_input(db, account, now).await? {
            roles.extend(input.roles);
        }
    }
    let approved: Vec<BoardRow> = sqlx::query_as(concat!(
        "SELECT ",
        crate::job_boards::board_columns!(),
        " FROM sources.job_boards WHERE kind = 'hiring_system' AND status = 'approved'
          ORDER BY system"
    ))
    .fetch_all(db)
    .await?;
    let systems: Vec<&System> = approved
        .iter()
        .filter_map(|row| row.system.as_deref().and_then(hiring_systems::named))
        .collect();
    let all = queries(&roles, &systems);
    let selected = this_weeks(&all, count, now);
    if selected.is_empty() {
        return Ok(summary);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(Error::internal)?;
    let mut endpoint = deps.config.search.brave_url.clone();
    endpoint.set_path("/res/v1/web/search");
    // Brave's free plan allows one query a second.
    let pause = if deps.config.allow_private_hosts() {
        Duration::ZERO
    } else {
        Duration::from_millis(1_100)
    };
    for (i, query) in selected.iter().enumerate() {
        if i > 0 && !pause.is_zero() {
            tokio::time::sleep(pause).await;
        }
        let mut url = endpoint.clone();
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("count", &RESULTS.to_string());
        let sent = client
            .get(url)
            .header("X-Subscription-Token", &key)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await;
        summary.queries += 1;
        let (results, added, stop) = match sent {
            Ok(response) if response.status().is_success() => {
                let answer: serde_json::Value = response.json().await.unwrap_or_default();
                let links = links_of(&answer);
                let added = crate::company_pages::register(db, &links, "web search", now).await?;
                summary.added += added;
                (links.len(), added, false)
            }
            Ok(response) => {
                // A key Brave refuses or a limit it holds: no more today.
                let status = response.status();
                tracing::info!(status = status.as_u16(), "the web search was refused");
                (0, 0, matches!(status.as_u16(), 401 | 402 | 403 | 429))
            }
            Err(_) => (0, 0, false),
        };
        sqlx::query(
            "INSERT INTO sources.web_search_queries (id, at, query, results, added)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(now)
        .bind(query.chars().take(300).collect::<String>())
        .bind(i32::try_from(results).unwrap_or(i32::MAX))
        .bind(i32::try_from(added).unwrap_or(i32::MAX))
        .execute(db)
        .await?;
        if stop {
            break;
        }
    }
    tracing::info!(
        queries = summary.queries,
        added = summary.added,
        "the weekly web search finished"
    );
    Ok(summary)
}
