//! Reading the store: what a search will call (#89, second part) instead of
//! reading the web. Module-internal; no endpoint, no model call.
//!
//! Only open jobs of approved sites (and of company pages whose hiring
//! system is approved) are read: a job its site no longer lists, one past
//! its expiry, and anything of a paused or declined site stay out of view.

use applicamp_kernel::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    job::{BoardJob, Job, Source, SourceLink, WorkMode},
    requirements::Salary,
};

/// Terms of a text query read at most.
pub const MAX_TERMS: usize = 40;
/// Jobs one read returns, at most.
pub const MAX_LIMIT: i64 = 5_000;

/// What a search asks of the store. Everything left out is not filtered.
#[derive(Debug, Clone, Copy)]
pub struct StoreQuery<'a> {
    /// Posted since (the day it was first seen, for a job that says none).
    pub posted_since: Option<DateTime<Utc>>,
    /// First stored since: the jobs new since the last nightly search.
    pub first_seen_since: Option<DateTime<Utc>>,
    /// Any of these ways of working; a job whose site says none passes.
    pub work_modes: &'a [WorkMode],
    /// Open to any of these places, as the site writes them (any case); a
    /// job whose site names no region passes.
    pub regions: &'a [String],
    /// Words of the person's roles, skills and kinds of jobs: a term is its
    /// words together, the terms alternatives. Looked for in the title, the
    /// company and the site's tags.
    pub text: &'a [String],
    /// Jobs returned, at most ([`MAX_LIMIT`]).
    pub limit: i64,
}

impl Default for StoreQuery<'_> {
    fn default() -> Self {
        Self {
            posted_since: None,
            first_seen_since: None,
            work_modes: &[],
            regions: &[],
            text: &[],
            limit: 500,
        }
    }
}

/// A job read from the store.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredJob {
    pub id: Uuid,
    pub board_id: Uuid,
    pub source_ref: String,
    pub title: String,
    pub company: String,
    pub url: String,
    pub apply_url: Option<String>,
    pub company_url: Option<String>,
    pub location: Option<String>,
    pub regions: Vec<String>,
    pub work_mode: Option<String>,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<String>,
    pub salary_text: Option<String>,
    pub posted_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub description: String,
    pub language: Option<String>,
    pub tags: String,
    pub url_key: Option<String>,
    pub dedupe_key: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    /// The registry row's slug, kind and hiring system.
    pub board_slug: String,
    pub board_kind: String,
    pub board_system: Option<String>,
    /// How the site's jobs are credited ("From Himalayas").
    pub attribution: String,
}

impl StoredJob {
    /// The job as every reader gives it, so a search treats it as one it had
    /// just read: a company page's job as its hiring system's reader gave
    /// it, any other as a job of the job board, credited to it.
    pub fn into_job(self) -> Job {
        let system = (self.board_kind == "company_page")
            .then(|| self.board_system.as_deref().and_then(Source::parse))
            .flatten();
        let source = system.unwrap_or(Source::Board);
        let pay = (self.salary_min.is_some() || self.salary_max.is_some()).then(|| Salary {
            min: self.salary_min,
            max: self.salary_max,
            currency: self.salary_currency.clone(),
            period: self.salary_period.clone(),
        });
        let board = system.is_none().then(|| BoardJob {
            slug: self.board_slug.clone(),
            links: vec![SourceLink {
                board: self.board_slug.clone(),
                name: self.attribution.clone(),
                url: self.url.clone(),
            }],
            apply_url: self.apply_url.clone(),
            company_url: self.company_url.clone(),
            regions: self.regions.clone(),
            expires_at: self.expires_at,
            pay,
        });
        Job {
            source,
            source_ref: self.source_ref,
            title: self.title,
            company: self.company,
            location: self.location,
            work_mode: self.work_mode.as_deref().and_then(WorkMode::parse),
            board_remote: None,
            url: self.url,
            posted_at: self.posted_at,
            salary_text: self.salary_text,
            description: self.description,
            board,
        }
    }
}

/// `terms` as a text-search query: each term's words and-ed, the terms
/// or-ed (`(senior & rust) | (platform)`); `None` when no word is left.
/// Only letters and digits get through, so nothing of a term is an operator.
pub fn text_query(terms: &[String]) -> Option<String> {
    let phrases: Vec<String> = terms
        .iter()
        .filter_map(|term| {
            let words: Vec<String> = term
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect();
            (!words.is_empty()).then(|| format!("({})", words.join(" & ")))
        })
        .take(MAX_TERMS)
        .collect();
    (!phrases.is_empty()).then(|| phrases.join(" | "))
}

/// The open jobs of approved sites that match `query`, best match first (the
/// newest first without a text), at most `query.limit`.
pub async fn find(
    db: &PgPool,
    query: &StoreQuery<'_>,
    now: DateTime<Utc>,
) -> Result<Vec<StoredJob>> {
    let modes: Vec<&str> = query.work_modes.iter().map(|m| m.as_str()).collect();
    let regions: Vec<String> = query.regions.iter().map(|r| r.to_lowercase()).collect();
    let text = text_query(query.text);
    Ok(sqlx::query_as(
        "SELECT j.id, j.board_id, j.source_ref, j.title, j.company, j.url, j.apply_url,
                j.company_url, j.location, j.regions, j.work_mode, j.salary_min, j.salary_max,
                j.salary_currency, j.salary_period, j.salary_text, j.posted_at, j.expires_at,
                j.description, j.language, j.tags, j.url_key, j.dedupe_key, j.first_seen_at,
                j.last_seen_at, b.slug AS board_slug, b.kind AS board_kind,
                b.system AS board_system,
                coalesce(nullif(btrim(b.terms->>'attribution'), ''), b.name) AS attribution
           FROM sources.store_jobs j
           JOIN sources.job_boards b ON b.id = j.board_id
           LEFT JOIN sources.job_boards s ON s.id = b.system_id
          WHERE j.closed_at IS NULL
            AND (j.expires_at IS NULL OR j.expires_at > $1)
            AND b.status = 'approved'
            AND (b.kind <> 'company_page' OR s.status = 'approved')
            AND ($2::timestamptz IS NULL OR coalesce(j.posted_at, j.first_seen_at) >= $2)
            AND ($3::timestamptz IS NULL OR j.first_seen_at >= $3)
            AND (cardinality($4::text[]) = 0 OR j.work_mode IS NULL OR j.work_mode = ANY($4))
            AND (cardinality($5::text[]) = 0 OR cardinality(j.regions) = 0
                 OR EXISTS (SELECT 1 FROM unnest(j.regions) r WHERE lower(r) = ANY($5)))
            AND ($6::text IS NULL OR j.search @@ to_tsquery('simple', $6))
          ORDER BY CASE WHEN $6::text IS NULL THEN 0::real
                        ELSE ts_rank(j.search, to_tsquery('simple', $6)) END DESC,
                   j.posted_at DESC NULLS LAST, j.first_seen_at DESC, j.id
          LIMIT $7",
    )
    .bind(now)
    .bind(query.posted_since)
    .bind(query.first_seen_since)
    .bind(&modes)
    .bind(&regions)
    .bind(text)
    .bind(query.limit.clamp(1, MAX_LIMIT))
    .fetch_all(db)
    .await?)
}

/// Jobs a search's pool holds, at most.
pub const POOL_MAX: i64 = 100_000;

/// What a search draws its candidates from.
#[derive(Debug, Clone, Copy)]
pub struct PoolQuery<'a> {
    /// The job sites the person has On; every approved company page is
    /// always in.
    pub sites: &'a [Uuid],
    /// First stored since: a nightly search takes only the jobs the store
    /// first saw after the person's last search.
    pub first_seen_since: Option<DateTime<Utc>>,
    /// Words of the person's roles and skills, for ranking only.
    pub text: &'a [String],
}

/// A job of the pool, without its description, and how close it is to the
/// person's roles and skills.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PoolJob {
    #[sqlx(flatten)]
    pub job: StoredJob,
    /// Full-text rank of the title, company and tags against the person's
    /// words (0 to 1), plus a little for how new it is. No model.
    pub rank: f32,
}

/// The open jobs of the person's On sites and of every approved company
/// page, with their rank. Descriptions are left out ([`descriptions`] reads
/// them for the jobs a search goes on with).
pub async fn pool(db: &PgPool, query: &PoolQuery<'_>, now: DateTime<Utc>) -> Result<Vec<PoolJob>> {
    let text = text_query(query.text);
    Ok(sqlx::query_as(
        "SELECT j.id, j.board_id, j.source_ref, j.title, j.company, j.url, j.apply_url,
                j.company_url, j.location, j.regions, j.work_mode, j.salary_min, j.salary_max,
                j.salary_currency, j.salary_period, j.salary_text, j.posted_at, j.expires_at,
                ''::text AS description, j.language, j.tags, j.url_key, j.dedupe_key,
                j.first_seen_at, j.last_seen_at, b.slug AS board_slug, b.kind AS board_kind,
                b.system AS board_system,
                coalesce(nullif(btrim(b.terms->>'attribution'), ''), b.name) AS attribution,
                (CASE WHEN $4::text IS NULL THEN 0::float8
                      ELSE ts_rank(j.search, to_tsquery('simple', $4), 32)::float8 END
                 + 0.3 / (1 + greatest(extract(epoch FROM ($1 - coalesce(j.posted_at, j.first_seen_at)))::float8
                                       / 86400.0, 0) / 7.0))::float4 AS rank
           FROM sources.store_jobs j
           JOIN sources.job_boards b ON b.id = j.board_id
           LEFT JOIN sources.job_boards s ON s.id = b.system_id
          WHERE j.closed_at IS NULL
            AND (j.expires_at IS NULL OR j.expires_at > $1)
            AND b.status = 'approved'
            AND ((b.kind <> 'company_page' AND b.id = ANY($2))
                 OR (b.kind = 'company_page' AND s.status = 'approved'))
            AND ($3::timestamptz IS NULL OR j.first_seen_at > $3)
          ORDER BY j.id
          LIMIT $5",
    )
    .bind(now)
    .bind(query.sites)
    .bind(query.first_seen_since)
    .bind(text)
    .bind(POOL_MAX)
    .fetch_all(db)
    .await?)
}

/// The descriptions of the stored jobs `ids`.
pub async fn descriptions(
    db: &PgPool,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, description FROM sources.store_jobs WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().collect())
}

/// About how many jobs of the pool `account` has no score for yet, for these
/// versions of the profile, goals and rules (no hard limit applied): what a
/// search's quick looks draw from.
pub async fn unscored(
    db: &PgPool,
    account: Uuid,
    sites: &[Uuid],
    versions: (&str, i32, i32),
    now: DateTime<Utc>,
) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*)
           FROM sources.store_jobs j
           JOIN sources.job_boards b ON b.id = j.board_id
           LEFT JOIN sources.job_boards s ON s.id = b.system_id
          WHERE j.closed_at IS NULL
            AND (j.expires_at IS NULL OR j.expires_at > $1)
            AND b.status = 'approved'
            AND ((b.kind <> 'company_page' AND b.id = ANY($2))
                 OR (b.kind = 'company_page' AND s.status = 'approved'))
            AND NOT EXISTS (
                SELECT 1 FROM sources.scores c
                 WHERE c.account_id = $3
                   AND c.source = CASE WHEN b.kind = 'company_page' THEN b.system ELSE b.slug END
                   AND c.source_ref = j.source_ref
                   AND c.profile_version = $4 AND c.goals_version = $5
                   AND c.rules_version = $6)",
    )
    .bind(now)
    .bind(sites)
    .bind(account)
    .bind(versions.0)
    .bind(versions.1)
    .bind(versions.2)
    .fetch_one(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_become_a_query_with_no_operator_of_the_persons() {
        let terms = [
            "Senior Rust engineer".to_owned(),
            "c++ / platform".to_owned(),
            "  ".to_owned(),
            "x & y | !z".to_owned(),
        ];
        assert_eq!(
            text_query(&terms).as_deref(),
            Some("(senior & rust & engineer) | (c & platform) | (x & y & z)")
        );
        assert_eq!(text_query(&["...".to_owned()]), None);
        assert_eq!(text_query(&[]), None);
    }
}
