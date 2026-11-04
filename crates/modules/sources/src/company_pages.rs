//! The registry's hiring systems and company pages (#87), kept in
//! `sources.job_boards` beside the job boards (kinds `hiring_system` and
//! `company_page`).
//!
//! - **Hiring systems**: one row each ([`ensure_systems`]), with the
//!   system-level check ([`crate::features::company_pages`]): robots rules,
//!   terms and content signal, decided once for every company on it.
//! - **Company pages**: one row per system and company, found through the
//!   links of jobs already read ([`note_links`]), a web search, a company
//!   someone watches or the built-in list. Registering one is cheap and
//!   batched: the address says it, no request is made, no model is called.
//!   A company page of a declined system is declined with it.

use applicamp_kernel::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    hiring_systems::{self, CompanyLink, SYSTEMS},
    job::{Job, Source},
    job_boards,
    readers::Board,
    robots,
};

/// Company pages one read of jobs registers, at most.
pub const MOST_PER_READ: usize = 200;

/// `token` as a company's name (`acme-labs` is `Acme Labs`).
pub fn name_of_token(token: &str) -> String {
    let name = token
        .split(['-', '_', '.'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    name.chars().take(100).collect()
}

/// The slug of a company page: its system, its token, and a short
/// fingerprint (two tokens that differ by a dot or a case stay apart).
pub fn page_slug(source: Source, token: &str, eu: bool) -> String {
    let system = source.as_str();
    let tag = robots::fingerprint(&format!("{system}:{}:{eu}", token.to_ascii_lowercase()));
    let tag = &tag[..6];
    let room = 40usize.saturating_sub(system.len() + 2 + 6);
    let base: String = job_boards::slug_of(token)
        .unwrap_or_default()
        .chars()
        .take(room)
        .collect();
    let base = base.trim_matches('-');
    if base.is_empty() {
        format!("{system}-{tag}")
    } else {
        format!("{system}-{base}-{tag}")
    }
}

/// Make the registry hold a row for every hiring system (new ones added as
/// candidates; known ones, and what was decided about them, left as they
/// are). How many rows there are.
pub async fn ensure_systems(db: &PgPool, now: DateTime<Utc>) -> Result<usize> {
    for system in &SYSTEMS {
        sqlx::query(
            "INSERT INTO sources.job_boards
                (id, slug, name, homepage, host, kind, priority, status, origin, config, terms,
                 system, found_through, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, 'hiring_system', 'low', 'candidate', 'seed', '{}', '{}',
                     $6, 'the list of hiring systems', $7, $7)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(system.slug)
        .bind(system.name)
        .bind(system.homepage)
        .bind(system.host)
        .bind(system.source.as_str())
        .bind(now)
        .execute(db)
        .await?;
    }
    Ok(SYSTEMS.len())
}

/// Register the company pages `links` name, as candidates (declined when
/// their system is). One statement, no request. How many were new.
pub async fn register(
    db: &PgPool,
    links: &[CompanyLink],
    found_through: &str,
    now: DateTime<Utc>,
) -> Result<usize> {
    if links.is_empty() {
        return Ok(0);
    }
    let links = &links[..links.len().min(MOST_PER_READ)];
    let ids: Vec<Uuid> = links.iter().map(|_| Uuid::now_v7()).collect();
    let slugs: Vec<String> = links
        .iter()
        .map(|l| page_slug(l.source, &l.token, l.eu))
        .collect();
    let names: Vec<String> = links.iter().map(|l| name_of_token(&l.token)).collect();
    let homepages: Vec<String> = links
        .iter()
        .map(|l| hiring_systems::page_url(l.source, &l.token, l.eu))
        .collect();
    let tokens: Vec<&str> = links.iter().map(|l| l.token.as_str()).collect();
    let systems: Vec<&str> = links.iter().map(|l| l.source.as_str()).collect();
    let eus: Vec<bool> = links.iter().map(|l| l.eu).collect();
    let added = sqlx::query(
        "INSERT INTO sources.job_boards
            (id, slug, name, homepage, host, kind, priority, status, origin, config, terms,
             system, company_token, eu, system_id, found_through, decline_reason, created_at,
             updated_at)
         SELECT c.id, c.slug, c.name, c.homepage, s.host, 'company_page', 'low',
                CASE WHEN s.status = 'declined' THEN 'declined' ELSE 'candidate' END,
                'discovery', '{}'::jsonb, '{}'::jsonb, c.system, c.token, c.eu, s.id, $8,
                CASE WHEN s.status = 'declined' THEN 'Its hiring system was declined.' END,
                $9, $9
           FROM unnest($1::uuid[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[],
                       $7::bool[]) AS c(id, slug, name, homepage, token, system, eu)
           JOIN sources.job_boards s ON s.kind = 'hiring_system' AND s.system = c.system
         ON CONFLICT DO NOTHING",
    )
    .bind(&ids)
    .bind(&slugs)
    .bind(&names)
    .bind(&homepages)
    .bind(&tokens)
    .bind(&systems)
    .bind(&eus)
    .bind(found_through.chars().take(300).collect::<String>())
    .bind(now)
    .execute(db)
    .await?
    .rows_affected();
    Ok(usize::try_from(added).unwrap_or(usize::MAX))
}

/// Register the company pages the links of `jobs` name (an apply link on
/// `site`). A failure is logged, never raised: reading jobs does not depend
/// on it.
pub async fn note_links(db: &PgPool, jobs: &[Job], site: &str, now: DateTime<Utc>) {
    let links = hiring_systems::company_links(jobs);
    if links.is_empty() {
        return;
    }
    let through = format!("an apply link on {site}");
    match register(db, &links, &through, now).await {
        Ok(added) if added > 0 => {
            tracing::info!(added, "company pages found through apply links");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "company pages could not be registered"),
    }
}

/// Register the company boards of the built-in list (`sources.boards`).
pub async fn register_boards(db: &PgPool, boards: &[Board], now: DateTime<Utc>) -> Result<usize> {
    let links: Vec<CompanyLink> = boards
        .iter()
        .filter(|b| hiring_systems::of(b.kind).is_some())
        .map(|b| CompanyLink {
            source: b.kind,
            token: b.token.clone(),
            eu: b.eu,
        })
        .collect();
    let mut added = 0;
    for chunk in links.chunks(MOST_PER_READ) {
        added += register(db, chunk, "the built-in list of companies", now).await?;
    }
    Ok(added)
}

/// A watched company's board is a company page: register it when it is not
/// there yet ("your company watch"), and link the company to it. The page's
/// id, when there is one.
pub async fn link_company(
    db: &PgPool,
    company: Uuid,
    link: &CompanyLink,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>> {
    register(db, std::slice::from_ref(link), "your company watch", now).await?;
    let page: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sources.job_boards
          WHERE kind = 'company_page' AND system = $1 AND lower(company_token) = lower($2)
            AND eu = $3",
    )
    .bind(link.source.as_str())
    .bind(&link.token)
    .bind(link.eu)
    .fetch_optional(db)
    .await?;
    if let Some(page) = page {
        sqlx::query("UPDATE sources.companies SET page_id = $2 WHERE id = $1")
            .bind(company)
            .bind(page)
            .execute(db)
            .await?;
    }
    Ok(page)
}

/// A watch read its company's board: keep that on the company page.
pub async fn note_read(db: &PgPool, page: Uuid, jobs: usize, now: DateTime<Utc>) -> Result<()> {
    job_boards::record_fetch(db, page, now, Some(jobs), None, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_becomes_a_name_and_a_slug() {
        assert_eq!(name_of_token("acme-labs"), "Acme Labs");
        assert_eq!(name_of_token("Acme.Co"), "Acme Co");
        let slug = page_slug(Source::Lever, "Acme.Co", false);
        assert!(slug.starts_with("lever-acme-co-"), "{slug}");
        assert!(job_boards::is_slug(&slug), "{slug}");
        assert_ne!(
            page_slug(Source::Lever, "acme-co", false),
            slug,
            "a dot and a dash stay apart"
        );
        let long = page_slug(Source::Teamtailor, &"a".repeat(99), false);
        assert!(long.len() <= 40 && job_boards::is_slug(&long), "{long}");
        assert_eq!(
            page_slug(Source::Lever, "ACME", false),
            page_slug(Source::Lever, "acme", false)
        );
    }
}
