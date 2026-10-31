//! A past employer's facts, for the profile wizard "Tell me about you" (#43).
//!
//! The same mechanism as a job's company: the company is the same record
//! (`pipeline.companies`, by name), its site is chosen the same way (a
//! person-typed one wins), its pages are read by the same reader, its facts
//! are made by the same call and checked the same way (a fact whose quote is
//! not on the page it cites is left out), cached 30 days, counted in the same
//! daily cap. Nothing but the employer's own site is fetched, and only where
//! robots.txt allows; an employer with no known site is not read at all.

use applicamp_kernel::{AppState, Error, Result};
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use super::{FRESH_DAYS, fetch_and_store, neighbours};
use crate::{
    PipelineDeps,
    companies::{self, SiteSource},
};

/// A fact read on an employer's own site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployerFact {
    /// The fact.
    pub text: String,
    /// The page's own words that say it (verbatim on the page).
    pub quote: String,
    /// The page it was read on.
    pub url: String,
}

/// What is known about an employer from its own site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmployerFacts {
    /// The site, when one is known.
    pub site: Option<String>,
    /// This call read the pages (one model call); false when the facts were
    /// fresh, or there was no site.
    pub read: bool,
    /// The facts the person has not removed.
    pub facts: Vec<EmployerFact>,
}

/// The site of the company named `name`, when one is known.
pub async fn site_of(db: &PgPool, account: Uuid, name: &str) -> Result<Option<String>> {
    Ok(companies::find(db, account, name)
        .await?
        .and_then(|company| company.site))
}

/// Record `raw` (an address) as `name`'s own site, typed by the person.
pub async fn set_site(
    deps: &PipelineDeps,
    state: &AppState,
    account: Uuid,
    name: &str,
    raw: &str,
) -> Result<String> {
    let invalid = || {
        Error::BadRequest(
            "invalid_site",
            "Write the company's website, starting with https://.".to_owned(),
        )
    };
    let site = companies::origin_of(raw).ok_or_else(invalid)?;
    if !neighbours(deps)?.site_allowed(&site) {
        return Err(Error::BadRequest(
            "site_not_allowed",
            "That address cannot be read. Use the company's public website.".to_owned(),
        ));
    }
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    companies::note_site(&mut tx, account, name, &site, SiteSource::Person, now).await?;
    tx.commit().await?;
    Ok(site.to_string())
}

/// The facts of the company named `name`: read from its site if it is known
/// and they are not fresh (**one model call**), else the ones kept. Without a
/// site nothing is fetched and there are no facts.
pub async fn facts(
    deps: &PipelineDeps,
    state: &AppState,
    account: Uuid,
    name: &str,
    now: DateTime<Utc>,
) -> Result<EmployerFacts> {
    let db = &state.db;
    let Some(company) = companies::find(db, account, name).await? else {
        return Ok(EmployerFacts::default());
    };
    let Some(site) = company.site.as_deref().and_then(|s| Url::parse(s).ok()) else {
        return Ok(EmployerFacts::default());
    };
    let fresh = company
        .read_at
        .is_some_and(|at| now < at + TimeDelta::days(FRESH_DAYS));
    if !fresh {
        fetch_and_store(
            deps,
            state,
            account,
            name,
            &site,
            company.source().unwrap_or(SiteSource::Person),
            None,
            None,
            now,
        )
        .await?;
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT f.text, f.quote, f.url FROM pipeline.company_facts f
          JOIN pipeline.companies c ON c.id = f.company_id
         WHERE c.account_id = $1 AND c.key = $2 AND NOT f.removed
         ORDER BY f.created_at, f.id",
    )
    .bind(account)
    .bind(companies::key_of(name))
    .fetch_all(db)
    .await?;
    Ok(EmployerFacts {
        site: Some(site.to_string()),
        read: !fresh,
        facts: rows
            .into_iter()
            .map(|(text, quote, url)| EmployerFact { text, quote, url })
            .collect(),
    })
}
