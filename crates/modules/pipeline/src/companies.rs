//! A company, per account: its own website (asked once), and when its pages
//! were last read. The company is named the way the opportunity names it; two
//! spellings that differ only in case and blanks are one company.

use applicamp_kernel::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use url::Url;
use uuid::Uuid;

use crate::store::MAX_SHORT;

/// Where a company's site came from. A site the person typed wins over the
/// others; a board's wins over an apply link's host, which wins over one
/// found in the posting, which wins over one found from the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteSource {
    /// The job board gave the company's website.
    Board,
    /// The apply link's host, which is not a known job board or tracking
    /// system.
    ApplyHost,
    /// The person typed it.
    Person,
    /// A link in the posting.
    PostingLink,
    /// An email address in the posting.
    PostingEmail,
    /// A domain made from the company's name, whose home page names it.
    NameGuess,
}

impl SiteSource {
    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::ApplyHost => "apply_host",
            Self::Person => "person",
            Self::PostingLink => "posting_link",
            Self::PostingEmail => "posting_email",
            Self::NameGuess => "name_guess",
        }
    }

    /// The source stored as `raw`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "board" => Some(Self::Board),
            "apply_host" => Some(Self::ApplyHost),
            "person" => Some(Self::Person),
            "posting_link" => Some(Self::PostingLink),
            "posting_email" => Some(Self::PostingEmail),
            "name_guess" => Some(Self::NameGuess),
            _ => None,
        }
    }

    /// How much it is trusted: a site is replaced only by one trusted as
    /// much or more.
    pub const fn rank(self) -> i32 {
        match self {
            Self::Person => 5,
            Self::Board => 4,
            Self::ApplyHost => 3,
            Self::PostingLink | Self::PostingEmail => 2,
            Self::NameGuess => 1,
        }
    }
}

/// A page a read fetched, as the company remembers it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadPage {
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    /// `posting`, `home` or `linked`.
    pub kind: String,
}

/// A company as stored.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Company {
    pub id: Uuid,
    pub name: String,
    pub site: Option<String>,
    pub site_source: Option<String>,
    pub read_at: Option<DateTime<Utc>>,
    pub pages: sqlx::types::Json<Vec<ReadPage>>,
    pub pages_blocked: i32,
    pub facts_dropped: i32,
    /// Why the last read of its site failed, when it did:
    /// `robots_disallows`, `no_pages_readable` or `site_unreachable`.
    pub read_failure: Option<String>,
}

impl Company {
    /// Where its site came from.
    pub fn source(&self) -> Option<SiteSource> {
        self.site_source.as_deref().and_then(SiteSource::parse)
    }
}

/// The company named `name`, as a key: lower case, blanks squeezed.
pub fn key_of(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(MAX_SHORT)
        .collect()
}

/// `raw` as a company's site: an http(s) origin without credentials, path,
/// query or fragment; `None` for anything else.
pub fn origin_of(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    let mut origin = Url::parse(&format!("{}://{host}/", url.scheme())).ok()?;
    if let Some(port) = url.port() {
        origin.set_port(Some(port)).ok()?;
    }
    Some(origin)
}

/// `account`'s company named `name`, if it is on record.
pub async fn find(db: &PgPool, account: Uuid, name: &str) -> Result<Option<Company>> {
    Ok(sqlx::query_as(
        "SELECT id, name, site, site_source, read_at, pages, pages_blocked, facts_dropped,
                read_failure
           FROM pipeline.companies WHERE account_id = $1 AND key = $2",
    )
    .bind(account)
    .bind(key_of(name))
    .fetch_optional(db)
    .await?)
}

/// `account`'s company named `name`, on record from now on.
pub async fn ensure(
    conn: &mut PgConnection,
    account: Uuid,
    name: &str,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let name = name.trim();
    Ok(sqlx::query_scalar(
        "INSERT INTO pipeline.companies (id, account_id, key, name, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $5)
         ON CONFLICT (account_id, key) DO UPDATE SET updated_at = pipeline.companies.updated_at
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(account)
    .bind(key_of(name))
    .bind(name.chars().take(MAX_SHORT).collect::<String>())
    .bind(now)
    .fetch_one(conn)
    .await?)
}

/// Record `site` (an origin) as the company's, unless a better source is
/// already there ([`SiteSource::rank`]): the person's own always stays; a
/// board's replaces an apply host's, which replaces a found one.
pub async fn note_site(
    conn: &mut PgConnection,
    account: Uuid,
    name: &str,
    site: &Url,
    source: SiteSource,
    now: DateTime<Utc>,
) -> Result<()> {
    if key_of(name).is_empty() {
        return Ok(());
    }
    let id = ensure(conn, account, name, now).await?;
    let stored: Option<String> =
        sqlx::query_scalar("SELECT site_source FROM pipeline.companies WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
    let stored = stored.as_deref().and_then(SiteSource::parse);
    if stored.is_some_and(|stored| stored.rank() > source.rank()) {
        return Ok(());
    }
    sqlx::query(
        "UPDATE pipeline.companies
            SET site = $2, site_source = $3, site_sought_at = NULL, updated_at = $4
          WHERE id = $1",
    )
    .bind(id)
    .bind(site.as_str())
    .bind(source.as_str())
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether `account`'s company named `name` had its site looked for, and not
/// found, since `since`.
pub async fn sought_since(
    db: &PgPool,
    account: Uuid,
    name: &str,
    since: DateTime<Utc>,
) -> Result<bool> {
    let at: Option<Option<DateTime<Utc>>> = sqlx::query_scalar(
        "SELECT site_sought_at FROM pipeline.companies WHERE account_id = $1 AND key = $2",
    )
    .bind(account)
    .bind(key_of(name))
    .fetch_optional(db)
    .await?;
    Ok(at.flatten().is_some_and(|at| at >= since))
}

/// Remember that `account`'s company named `name` had its site looked for at
/// `now` and not found.
pub async fn note_sought(db: &PgPool, account: Uuid, name: &str, now: DateTime<Utc>) -> Result<()> {
    if key_of(name).is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    let id = ensure(&mut tx, account, name, now).await?;
    sqlx::query("UPDATE pipeline.companies SET site_sought_at = $2 WHERE id = $1 AND site IS NULL")
        .bind(id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_company_is_its_name_in_lower_case_with_blanks_squeezed() {
        assert_eq!(key_of("  Example   Co "), "example co");
        assert_eq!(key_of("EXAMPLE CO"), key_of("Example Co"));
        assert_eq!(key_of("   "), "");
    }

    #[test]
    fn a_site_is_an_origin_and_nothing_more() {
        let site = |raw: &str| origin_of(raw).map(|u| u.to_string());
        assert_eq!(
            site("https://www.example.com/about?x=1#top").as_deref(),
            Some("https://www.example.com/")
        );
        assert_eq!(
            site("http://example.test:9020/jobs/1").as_deref(),
            Some("http://example.test:9020/")
        );
        assert_eq!(site("https://user:pw@example.com/"), None);
        assert_eq!(site("ftp://example.com/"), None);
        assert_eq!(site("not a link"), None);
    }

    #[test]
    fn the_person_wins_then_the_board_then_the_apply_host_then_what_was_found() {
        use SiteSource::*;
        let order = [Person, Board, ApplyHost, PostingLink, NameGuess];
        assert!(order.windows(2).all(|w| w[0].rank() > w[1].rank()));
        assert_eq!(PostingLink.rank(), PostingEmail.rank());
        for source in [
            Board,
            ApplyHost,
            Person,
            PostingLink,
            PostingEmail,
            NameGuess,
        ] {
            assert_eq!(SiteSource::parse(source.as_str()), Some(source));
        }
    }
}
