//! The watchers' records: companies (shared, they describe a company and its
//! job board), one person's watch of a company, and the postings seen on a
//! watched board.

use applicamp_kernel::Result;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{detect::BoardKind, suggest::CompanyReason};

/// A watch ending its search for a board is taken as dead after this long.
pub const FINDING_DEAD: TimeDelta = TimeDelta::minutes(10);

/// A watch of a company by an account, with the company.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Row {
    pub watch: Uuid,
    pub company: Uuid,
    pub account: Uuid,
    pub state: String,
    pub reasons: sqlx::types::Json<Vec<CompanyReason>>,
    pub created_at: DateTime<Utc>,
    pub watching_since: Option<DateTime<Utc>>,
    pub finding_since: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub open_jobs: Option<i32>,
    pub seeded_at: Option<DateTime<Utc>>,
    pub name: String,
    pub key: String,
    pub website: Option<String>,
    pub careers_url: Option<String>,
    pub board_kind: String,
    pub board_token: Option<String>,
    pub board_eu: bool,
    pub detected_at: Option<DateTime<Utc>>,
    pub origin: String,
    /// Its board as a company page of the registry, when it is one.
    pub page_id: Option<Uuid>,
}

impl Row {
    /// How its jobs are read.
    pub fn kind(&self) -> BoardKind {
        BoardKind::parse(&self.board_kind)
    }

    /// A board is being looked for.
    pub fn finding(&self, now: DateTime<Utc>) -> bool {
        self.finding_since.is_some_and(|at| now - at < FINDING_DEAD)
    }
}

/// A watch with its company, then `$tail` (a `WHERE` and what follows).
macro_rules! select {
    ($tail:literal) => {
        concat!(
            "SELECT w.id AS watch, c.id AS company, w.account_id AS account, w.state,
                    w.reasons, w.created_at, w.watching_since, w.finding_since,
                    w.last_checked_at, w.last_error, w.open_jobs, w.seeded_at, c.name, c.key,
                    c.website, c.careers_url, c.board_kind, c.board_token, c.board_eu,
                    c.detected_at, c.origin, c.page_id
               FROM sources.company_watches w JOIN sources.companies c ON c.id = w.company_id ",
            $tail
        )
    };
}

/// `account`'s suggestions and watches (the ones that show).
pub async fn shown(db: &PgPool, account: Uuid) -> Result<Vec<Row>> {
    Ok(sqlx::query_as(select!(
        "WHERE w.account_id = $1 AND w.state IN ('suggested', 'watching')
          ORDER BY w.watching_since NULLS LAST, w.created_at, w.id"
    ))
    .bind(account)
    .fetch_all(db)
    .await?)
}

/// `account`'s watch of `company`, if it has one.
pub async fn of_company(db: &PgPool, account: Uuid, company: Uuid) -> Result<Option<Row>> {
    Ok(
        sqlx::query_as(select!("WHERE w.account_id = $1 AND c.id = $2"))
            .bind(account)
            .bind(company)
            .fetch_optional(db)
            .await?,
    )
}

/// A watch by its id.
pub async fn by_watch(db: &PgPool, watch: Uuid) -> Result<Option<Row>> {
    Ok(sqlx::query_as(select!("WHERE w.id = $1"))
        .bind(watch)
        .fetch_optional(db)
        .await?)
}

/// How many companies `account` watches.
pub async fn watching(db: &PgPool, account: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM sources.company_watches WHERE account_id = $1 AND state = 'watching'",
    )
    .bind(account)
    .fetch_one(db)
    .await?)
}

/// A company as it is described.
#[derive(Debug, Clone)]
pub struct NewCompany<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub website: Option<&'a str>,
    pub careers_url: Option<&'a str>,
    pub origin: &'a str,
    /// A board a posting came from, to confirm when the company is watched.
    pub hint: Option<(BoardKind, &'a str, bool)>,
}

/// The company with `key`, made if it is new; a board already detected is
/// never replaced, a hint only fills a company with no board yet. Its id.
pub async fn ensure_company(
    conn: &mut PgConnection,
    company: &NewCompany<'_>,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let (kind, token, eu) = company.hint.map_or(("none", None, false), |(k, t, eu)| {
        (k.as_str(), Some(t), eu)
    });
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sources.companies
            (id, key, name, website, careers_url, board_kind, board_token, board_eu, origin,
             created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (key) DO UPDATE SET
            website = coalesce(sources.companies.website, excluded.website),
            careers_url = coalesce(excluded.careers_url, sources.companies.careers_url),
            board_kind = CASE WHEN sources.companies.board_kind = 'none'
                              THEN excluded.board_kind ELSE sources.companies.board_kind END,
            board_token = CASE WHEN sources.companies.board_kind = 'none'
                               THEN excluded.board_token ELSE sources.companies.board_token END,
            board_eu = CASE WHEN sources.companies.board_kind = 'none'
                            THEN excluded.board_eu ELSE sources.companies.board_eu END
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(company.key)
    .bind(company.name.chars().take(300).collect::<String>())
    .bind(company.website)
    .bind(company.careers_url)
    .bind(kind)
    .bind(token)
    .bind(eu)
    .bind(company.origin)
    .bind(now)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Make `account` suggested `company` for `reasons` (a company the person
/// already watches, stopped or dismissed stays as it is). Whether it is a
/// suggestion now.
pub async fn suggest(
    conn: &mut PgConnection,
    account: Uuid,
    company: Uuid,
    reasons: &[CompanyReason],
    now: DateTime<Utc>,
) -> Result<bool> {
    let suggested: Option<String> = sqlx::query_scalar(
        "INSERT INTO sources.company_watches
            (id, account_id, company_id, state, reasons, created_at, updated_at)
         VALUES ($1, $2, $3, 'suggested', $4, $5, $5)
         ON CONFLICT (account_id, company_id) DO UPDATE
            SET reasons = excluded.reasons, updated_at = excluded.updated_at
          WHERE sources.company_watches.state = 'suggested'
         RETURNING state",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(company)
    .bind(sqlx::types::Json(reasons))
    .bind(now)
    .fetch_optional(conn)
    .await?;
    Ok(suggested.is_some())
}

/// The suggestions of `account` that are not in `keep` any more (their
/// matches aged out): removed.
pub async fn drop_stale_suggestions(
    conn: &mut PgConnection,
    account: Uuid,
    keep: &[Uuid],
) -> Result<u64> {
    Ok(sqlx::query(
        "DELETE FROM sources.company_watches
          WHERE account_id = $1 AND state = 'suggested' AND NOT (company_id = ANY($2))",
    )
    .bind(account)
    .bind(keep)
    .execute(conn)
    .await?
    .rows_affected())
}

/// When `account`'s suggestions were last worked out.
pub async fn refreshed_at(db: &PgPool, account: Uuid) -> Result<Option<DateTime<Utc>>> {
    Ok(
        sqlx::query_scalar("SELECT at FROM sources.watch_refreshes WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?,
    )
}

/// Note that `account`'s suggestions were worked out at `now`.
pub async fn set_refreshed(
    conn: &mut PgConnection,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources.watch_refreshes (account_id, at) VALUES ($1, $2)
         ON CONFLICT (account_id) DO UPDATE SET at = excluded.at",
    )
    .bind(account)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Start (or restart) `account`'s watch of `company`: watching from `now`,
/// looking for its board, nothing seen yet.
pub async fn start_watching(
    conn: &mut PgConnection,
    account: Uuid,
    company: Uuid,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sources.company_watches
            (id, account_id, company_id, state, created_at, updated_at, watching_since,
             finding_since)
         VALUES ($1, $2, $3, 'watching', $4, $4, $4, $4)
         ON CONFLICT (account_id, company_id) DO UPDATE SET
            state = 'watching', updated_at = excluded.updated_at,
            watching_since = excluded.watching_since, finding_since = excluded.finding_since,
            last_checked_at = NULL, last_error = NULL, open_jobs = NULL, seeded_at = NULL
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(company)
    .bind(now)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM sources.watch_seen WHERE watch_id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(id)
}

/// Mark the search for a board of `watch` as started.
pub async fn finding(conn: &mut PgConnection, watch: Uuid, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "UPDATE sources.company_watches SET finding_since = $2, updated_at = $2 WHERE id = $1",
    )
    .bind(watch)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Set the state of `account`'s watch of `company`. Whether there was one.
pub async fn set_state(
    db: &PgPool,
    account: Uuid,
    company: Uuid,
    state: &str,
    now: DateTime<Utc>,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE sources.company_watches
            SET state = $3, updated_at = $4, finding_since = NULL
          WHERE account_id = $1 AND company_id = $2",
    )
    .bind(account)
    .bind(company)
    .bind(state)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

/// A board found for a company: kept on the company, and the watch's first
/// read kept on the watch (its postings noted as seen, nothing scored: the
/// watch is "from today").
pub async fn board_found(
    db: &PgPool,
    row: &Row,
    detected: &super::detect::Detected,
    seen: &[(String, String)],
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE sources.companies
            SET board_kind = $2, board_token = $3, board_eu = $4,
                careers_url = coalesce($5, careers_url),
                name = CASE WHEN origin = 'pasted' AND $7::text IS NOT NULL THEN $7 ELSE name END,
                detected_at = $6, checked_at = $6
          WHERE id = $1",
    )
    .bind(row.company)
    .bind(detected.kind.as_str())
    .bind(&detected.token)
    .bind(detected.eu)
    .bind(detected.careers_url.as_ref().map(url::Url::as_str))
    .bind(now)
    .bind(detected.name.as_deref())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE sources.company_watches
            SET finding_since = NULL, last_error = NULL, last_checked_at = $2, open_jobs = $3,
                seeded_at = $2, updated_at = $2
          WHERE id = $1",
    )
    .bind(row.watch)
    .bind(now)
    .bind(i32::try_from(detected.jobs.len()).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await?;
    note_seen(&mut tx, row.watch, seen, now).await?;
    tx.commit().await?;
    Ok(())
}

/// No board was found for a company.
pub async fn board_not_found(db: &PgPool, row: &Row, why: &str, now: DateTime<Utc>) -> Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE sources.companies
            SET board_kind = 'none', board_token = NULL, board_eu = false, checked_at = $2
          WHERE id = $1 AND detected_at IS NULL",
    )
    .bind(row.company)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE sources.company_watches
            SET finding_since = NULL, last_error = $2, updated_at = $3
          WHERE id = $1",
    )
    .bind(row.watch)
    .bind(why)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The watches due for a read: watching, with a board, and not read since
/// `before`.
pub async fn due(db: &PgPool, account: Uuid, before: DateTime<Utc>) -> Result<Vec<Row>> {
    Ok(sqlx::query_as(select!(
        "WHERE w.account_id = $1 AND w.state = 'watching' AND c.board_kind <> 'none'
            AND w.finding_since IS NULL
            AND (w.last_checked_at IS NULL OR w.last_checked_at <= $2)
          ORDER BY w.watching_since, w.id"
    ))
    .bind(account)
    .bind(before)
    .fetch_all(db)
    .await?)
}

/// Whether `account` has a watch due for a read.
pub async fn any_due(db: &PgPool, account: Uuid, before: DateTime<Utc>) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM sources.company_watches w
              JOIN sources.companies c ON c.id = w.company_id
             WHERE w.account_id = $1 AND w.state = 'watching' AND c.board_kind <> 'none'
               AND w.finding_since IS NULL
               AND (w.last_checked_at IS NULL OR w.last_checked_at <= $2))",
    )
    .bind(account)
    .bind(before)
    .fetch_one(db)
    .await?)
}

/// The postings already seen on a watch's board.
pub async fn seen(db: &PgPool, watch: Uuid) -> Result<std::collections::HashSet<(String, String)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT source, source_ref FROM sources.watch_seen WHERE watch_id = $1")
            .bind(watch)
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().collect())
}

/// Note `keys` as seen on `watch`'s board.
pub async fn note_seen(
    conn: &mut PgConnection,
    watch: Uuid,
    keys: &[(String, String)],
    now: DateTime<Utc>,
) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let sources: Vec<&str> = keys.iter().map(|k| k.0.as_str()).collect();
    let refs: Vec<&str> = keys.iter().map(|k| k.1.as_str()).collect();
    sqlx::query(
        "INSERT INTO sources.watch_seen (watch_id, source, source_ref, first_seen_at, last_seen_at)
         SELECT $1::uuid, s, r, $4::timestamptz, $4::timestamptz
           FROM unnest($2::text[], $3::text[]) AS t(s, r)
         ON CONFLICT (watch_id, source, source_ref) DO UPDATE SET last_seen_at = excluded.last_seen_at",
    )
    .bind(watch)
    .bind(&sources)
    .bind(&refs)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Keep what a read of `watch`'s board gave: when, how many jobs are open,
/// what went wrong; the postings still listed are seen again, and the ones
/// gone for longer than [`KEEP_SEEN`] are forgotten.
pub async fn read_done(
    db: &PgPool,
    watch: Uuid,
    listed: &[(String, String)],
    error: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = db.begin().await?;
    if error.is_none() {
        let sources: Vec<&str> = listed.iter().map(|k| k.0.as_str()).collect();
        let refs: Vec<&str> = listed.iter().map(|k| k.1.as_str()).collect();
        sqlx::query(
            "UPDATE sources.watch_seen SET last_seen_at = $4
              WHERE watch_id = $1
                AND (source, source_ref) IN (SELECT s, r FROM unnest($2::text[], $3::text[]) AS t(s, r))",
        )
        .bind(watch)
        .bind(&sources)
        .bind(&refs)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM sources.watch_seen WHERE watch_id = $1 AND last_seen_at < $2")
            .bind(watch)
            .bind(now - KEEP_SEEN)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE sources.company_watches
            SET last_checked_at = $2, last_error = $3,
                open_jobs = CASE WHEN $3::text IS NULL THEN $4 ELSE open_jobs END,
                seeded_at = coalesce(seeded_at, CASE WHEN $3::text IS NULL THEN $2 END),
                updated_at = $2
          WHERE id = $1",
    )
    .bind(watch)
    .bind(now)
    .bind(error)
    .bind(i32::try_from(listed.len()).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// A posting that is no longer listed is forgotten after this long.
pub const KEEP_SEEN: TimeDelta = TimeDelta::days(60);
