//! The boards searched: the seed list (`boards.toml`, or the file
//! `APPLICAMP_SOURCES__BOARDS_FILE` names) and the boards people added by
//! pasting a posting's link.

use applicamp_kernel::Result;
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::SourcesConfig,
    job::Source,
    readers::{Board, is_token},
};

/// The seed list built in.
pub const SEED: &str = include_str!("../boards.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    board: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    kind: String,
    token: String,
    company: String,
    #[serde(default)]
    eu: bool,
}

/// The boards in a seed list. Every entry must be a Greenhouse, Lever or
/// Ashby board with a safe token and a company; the first one that is not
/// is the error.
pub fn parse(toml_text: &str) -> std::result::Result<Vec<Board>, String> {
    let file: File = toml::from_str(toml_text).map_err(|e| format!("boards list: {e}"))?;
    let mut boards = Vec::with_capacity(file.board.len());
    for entry in file.board {
        let kind = Source::parse(&entry.kind)
            .filter(|kind| matches!(kind, Source::Greenhouse | Source::Lever | Source::Ashby))
            .ok_or_else(|| {
                format!(
                    "boards list: `{}` is not greenhouse, lever or ashby",
                    entry.kind
                )
            })?;
        if !is_token(&entry.token) {
            return Err(format!(
                "boards list: `{}` is not a board token",
                entry.token
            ));
        }
        let company = entry.company.trim().to_owned();
        if company.is_empty() {
            return Err(format!("boards list: `{}` has no company", entry.token));
        }
        if entry.eu && kind != Source::Lever {
            return Err(format!(
                "boards list: `{}`: only Lever boards are on an EU instance",
                entry.token
            ));
        }
        boards.push(Board {
            kind,
            token: entry.token,
            company,
            eu: entry.eu,
        });
    }
    Ok(boards)
}

/// The seed list `config` names: its file, or the one built in.
pub fn seed(config: &SourcesConfig) -> std::result::Result<Vec<Board>, String> {
    match &config.boards_file {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("APPLICAMP_SOURCES__BOARDS_FILE={}: {e}", path.display()))?;
            parse(&text)
        }
        None => parse(SEED),
    }
}

/// Make `sources.boards` hold `seed`: new ones added, ones no longer in it
/// removed (boards people added stay). How many seed boards there are.
pub async fn sync(db: &PgPool, seed: &[Board]) -> Result<usize> {
    let mut tx = db.begin().await?;
    let kinds: Vec<&str> = seed.iter().map(|b| b.kind.as_str()).collect();
    let tokens: Vec<&str> = seed.iter().map(|b| b.token.as_str()).collect();
    sqlx::query(
        "DELETE FROM sources.boards b
          WHERE added_by = 'seed'
            AND NOT EXISTS (SELECT 1 FROM unnest($1::text[], $2::text[]) AS s(kind, token)
                             WHERE s.kind = b.kind AND s.token = b.token)",
    )
    .bind(&kinds)
    .bind(&tokens)
    .execute(&mut *tx)
    .await?;
    for board in seed {
        sqlx::query(
            "INSERT INTO sources.boards (id, kind, token, company, eu, added_by, created_at)
             VALUES ($1, $2, $3, $4, $5, 'seed', $6)
             ON CONFLICT (kind, token) DO UPDATE SET company = excluded.company, eu = excluded.eu",
        )
        .bind(Uuid::now_v7())
        .bind(board.kind.as_str())
        .bind(&board.token)
        .bind(&board.company)
        .bind(board.eu)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(seed.len())
}

/// Every board searched.
pub async fn all(db: &PgPool) -> Result<Vec<Board>> {
    let rows: Vec<(String, String, String, bool)> =
        sqlx::query_as("SELECT kind, token, company, eu FROM sources.boards ORDER BY kind, token")
            .fetch_all(db)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(kind, token, company, eu)| {
            Some(Board {
                kind: Source::parse(&kind)?,
                token,
                company,
                eu,
            })
        })
        .collect())
}

/// The company of a known board.
pub async fn company(db: &PgPool, kind: Source, token: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT company FROM sources.boards WHERE kind = $1 AND lower(token) = lower($2)",
    )
    .bind(kind.as_str())
    .bind(token)
    .fetch_optional(db)
    .await?)
}

/// Add `board`, which `account` pasted a posting of, to the boards searched.
pub async fn add(db: &PgPool, board: &Board, account: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources.boards (id, kind, token, company, eu, added_by, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (kind, token) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(board.kind.as_str())
    .bind(&board.token)
    .bind(&board.company)
    .bind(board.eu)
    .bind(account.to_string())
    .bind(Utc::now())
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_list_parses_and_has_about_a_hundred_boards() {
        let boards = parse(SEED).unwrap();
        assert!(boards.len() >= 80, "{} boards", boards.len());
        let mut keys: Vec<_> = boards
            .iter()
            .map(|b| (b.kind, b.token.to_lowercase()))
            .collect();
        keys.sort_by(|a, b| (a.0.as_str(), &a.1).cmp(&(b.0.as_str(), &b.1)));
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "no board twice");
    }

    #[test]
    fn bad_entries_are_refused() {
        for bad in [
            "[[board]]\nkind = \"linkedin\"\ntoken = \"x\"\ncompany = \"X\"",
            "[[board]]\nkind = \"lever\"\ntoken = \"../x\"\ncompany = \"X\"",
            "[[board]]\nkind = \"ashby\"\ntoken = \"x\"\ncompany = \" \"",
            "[[board]]\nkind = \"ashby\"\ntoken = \"x\"\ncompany = \"X\"\neu = true",
            "[[board]]\nkind = \"ashby\"\ntoken = \"x\"\ncompany = \"X\"\nhost = \"evil.example\"",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[sqlx::test(migrations = "../../../migrations")]
    async fn syncing_keeps_added_boards_and_follows_the_list(db: PgPool) {
        let board = |token: &str| Board {
            kind: Source::Greenhouse,
            token: token.into(),
            company: "Example Co".into(),
            eu: false,
        };
        sync(&db, &[board("one"), board("two")]).await.unwrap();
        add(&db, &board("pasted"), Uuid::now_v7()).await.unwrap();
        sync(&db, &[board("two"), board("three")]).await.unwrap();
        let tokens: Vec<String> = all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|b| b.token)
            .collect();
        assert_eq!(tokens, ["pasted", "three", "two"]);
        assert_eq!(
            company(&db, Source::Greenhouse, "TWO")
                .await
                .unwrap()
                .as_deref(),
            Some("Example Co")
        );
    }
}
