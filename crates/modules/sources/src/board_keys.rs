//! The API keys some job boards need (ITJobs.pt, Adzuna): set by a board
//! owner on the board's page, stored sealed with the instance secret
//! (AES-256-GCM under its own purpose, bound to the board's id), sent only
//! as the query parameters the board's config names and only to the host it
//! was set for, never written to the cache, the logs, a report or an answer
//! to a client (which learns only that a key is set).

use std::collections::BTreeMap;

use applicamp_kernel::crypto::{Cipher, SecretKey};
use uuid::Uuid;

use crate::job_boards::{BoardConfig, BoardRow};

/// The sealing purpose of board keys. Never change it: keys sealed under it
/// could not be opened.
pub const PURPOSE: &[u8] = b"applicamp sources board key v1";

/// The longest value of one key parameter.
pub const MAX_VALUE: usize = 200;

/// The cipher for board keys under `secret`.
pub fn cipher(secret: &SecretKey) -> Cipher {
    Cipher::new(secret, PURPOSE)
}

fn context(board: Uuid) -> Vec<u8> {
    let mut context = b"sources.job_boards.api_key:".to_vec();
    context.extend_from_slice(board.as_bytes());
    context
}

/// `values` (parameter → value) sealed for `board`.
pub fn seal(cipher: &Cipher, board: Uuid, values: &BTreeMap<String, String>) -> Vec<u8> {
    let text = serde_json::to_string(values).unwrap_or_default();
    cipher.seal(&text, &context(board))
}

/// The values sealed for `board`; `None` when they cannot be opened (another
/// secret, another board, tampered).
pub fn open(cipher: &Cipher, board: Uuid, sealed: &[u8]) -> Option<BTreeMap<String, String>> {
    let text = cipher.open(sealed, &context(board)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Whether `value` may be sent as a key parameter: 1 to [`MAX_VALUE`]
/// visible characters.
pub fn is_value(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_VALUE
        && value.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// Why a board's key cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// None is set (or it was set for another host).
    NotSet,
    /// It is set but cannot be opened (the instance secret changed).
    Unreadable,
}

/// The query parameters to add to a request of `row` read with `config`:
/// none when its config names no key; its key's when one is set for the
/// host the config reads; else why not.
pub fn query(
    cipher: Option<&Cipher>,
    row: &BoardRow,
    config: &BoardConfig,
) -> Result<Vec<(String, String)>, Missing> {
    let Some(spec) = &config.key else {
        return Ok(Vec::new());
    };
    if !row.key_set_for(config) {
        return Err(Missing::NotSet);
    }
    let (Some(cipher), Some(sealed)) = (cipher, row.api_key.as_deref()) else {
        return Err(Missing::Unreadable);
    };
    let values = open(cipher, row.id, sealed).ok_or(Missing::Unreadable)?;
    spec.params
        .iter()
        .map(|param| {
            values
                .get(param)
                .filter(|v| is_value(v))
                .map(|v| (param.clone(), v.clone()))
                .ok_or(Missing::NotSet)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use applicamp_kernel::crypto::SECRET_KEY_LEN;

    use super::*;

    #[test]
    fn keys_open_only_for_their_board_and_secret() {
        let one = cipher(&SecretKey::from_bytes([1; SECRET_KEY_LEN]));
        let other = cipher(&SecretKey::from_bytes([2; SECRET_KEY_LEN]));
        let board = Uuid::now_v7();
        let values: BTreeMap<String, String> = [("api_key".to_owned(), "k-123".to_owned())].into();
        let sealed = seal(&one, board, &values);
        assert!(!String::from_utf8_lossy(&sealed).contains("k-123"));
        assert_eq!(open(&one, board, &sealed), Some(values));
        assert_eq!(open(&one, Uuid::now_v7(), &sealed), None);
        assert_eq!(open(&other, board, &sealed), None);
        assert!(is_value("abc-123_X"));
        assert!(!is_value("") && !is_value("a b") && !is_value("a\nb"));
        assert!(!is_value(&"x".repeat(MAX_VALUE + 1)));
    }
}
