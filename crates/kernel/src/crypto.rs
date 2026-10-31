//! Secrets the server makes and keeps: random tokens, their hashes, PKCE,
//! and the encryption of secrets at rest.
//!
//! - **The instance secret** ([`SecretKey`]) is 32 random bytes from
//!   `APPLICAMP_SECRET_KEY_FILE` or `APPLICAMP_SECRET_KEY` (standard base64).
//! - **Sealing** ([`Cipher`]): AES-256-GCM under a key derived (HKDF-SHA256)
//!   from the instance secret and a **purpose label**, so every use (the
//!   sign-in cookie, stored refresh tokens) has its own key. Each value gets
//!   a fresh random nonce, and the caller's context (table, column, row) is
//!   the associated data, so a ciphertext cannot be moved to another row.
//! - **Derived keys** ([`SecretKey::derive`]): 32 bytes (HKDF-SHA256) for
//!   one purpose label, e.g. the seed of the Ed25519 key the gateway trusts.
//! - Cookie tokens are 256-bit random values ([`random_token`]); the
//!   database keeps only their SHA-256 ([`hash`]), compared in constant time
//!   ([`same_hash`]).

use aws_lc_rs::{
    aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey},
    constant_time, digest, hkdf,
    rand::{SecureRandom as _, SystemRandom},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use secrecy::ExposeSecret as _;

use crate::config::{Config, ConfigError, read_secret};

/// Length of the instance secret once decoded.
pub const SECRET_KEY_LEN: usize = 32;

/// First byte of every sealed value, so the format can change later.
const FORMAT_V1: u8 = 1;

/// The instance secret. Never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey([u8; SECRET_KEY_LEN]);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

impl SecretKey {
    /// A key made of `bytes` (tests, or a key read elsewhere).
    pub const fn from_bytes(bytes: [u8; SECRET_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Decode standard base64 of 32 random bytes.
    pub fn decode(encoded: &str) -> Result<Self, String> {
        let bytes = STANDARD
            .decode(encoded.trim())
            .map_err(|_| "is not valid base64".to_owned())?;
        bytes.try_into().map(Self).map_err(|bytes: Vec<u8>| {
            format!(
                "decodes to {} bytes; it must be {SECRET_KEY_LEN} (e.g. `openssl rand -base64 32`)",
                bytes.len()
            )
        })
    }

    /// The instance secret `config` names: `APPLICAMP_SECRET_KEY_FILE`
    /// (preferred) or `APPLICAMP_SECRET_KEY`. An error when neither is set
    /// or the key is malformed; it names the variable, never the key.
    pub fn from_config(config: &Config) -> Result<Self, ConfigError> {
        let secret = read_secret(
            "APPLICAMP_SECRET_KEY",
            config.secret_key.as_ref(),
            config.secret_key_file.as_deref(),
        )?
        .ok_or_else(|| {
            ConfigError(
                "APPLICAMP_SECRET_KEY_FILE (or APPLICAMP_SECRET_KEY) is not set: generate one \
                 with `openssl rand -base64 32`"
                    .to_owned(),
            )
        })?;
        Self::decode(secret.expose_secret())
            .map_err(|why| ConfigError(format!("APPLICAMP_SECRET_KEY {why}")))
    }
}

/// The length of a key [`SecretKey::derive`] makes.
struct KeyLen;

impl hkdf::KeyType for KeyLen {
    fn len(&self) -> usize {
        SECRET_KEY_LEN
    }
}

impl SecretKey {
    /// 32 bytes derived (HKDF-SHA256) from the instance secret for
    /// `purpose`, a fixed, versioned label per use
    /// (`b"applicamp gateway oidc v1"`): the same secret and label give the
    /// same bytes, another label unrelated ones. Never change a label in use.
    pub fn derive(&self, purpose: &'static [u8]) -> [u8; SECRET_KEY_LEN] {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(&self.0);
        let info = [purpose];
        let mut out = [0_u8; SECRET_KEY_LEN];
        prk.expand(&info, KeyLen)
            .and_then(|okm| okm.fill(&mut out))
            .unwrap_or_else(|_| unreachable!("HKDF can expand to 32 bytes"));
        out
    }
}

/// A URL-safe random token of 256 bits.
pub fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    // The system RNG failing means the OS cannot provide entropy at all.
    SystemRandom::new()
        .fill(&mut bytes)
        .unwrap_or_else(|_| unreachable!("the system random number generator failed"));
    URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of `value`: what the database stores instead of a token.
pub fn hash(value: &str) -> Vec<u8> {
    digest::digest(&digest::SHA256, value.as_bytes())
        .as_ref()
        .to_vec()
}

/// Whether two byte strings are equal, in constant time.
pub fn same_hash(a: &[u8], b: &[u8]) -> bool {
    constant_time::verify_slices_are_equal(a, b).is_ok()
}

/// The PKCE S256 challenge of `verifier` (RFC 7636 §4.2).
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()))
}

/// A sealed value could not be opened: wrong key or tampered data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a sealed value could not be decrypted")]
pub struct OpenError;

/// Seals and opens secrets for one purpose.
pub struct Cipher {
    key: LessSafeKey,
    rng: SystemRandom,
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cipher(AES-256-GCM)")
    }
}

impl Cipher {
    /// The cipher for `purpose` under `secret`. `purpose` is the HKDF
    /// `info`: a fixed, versioned label per use
    /// (`b"applicamp bff login v1"`); never change a label in use, or what
    /// it sealed cannot be opened.
    pub fn new(secret: &SecretKey, purpose: &'static [u8]) -> Self {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(&secret.0);
        let info = [purpose];
        let okm = prk
            .expand(&info, &AES_256_GCM)
            .unwrap_or_else(|_| unreachable!("HKDF can expand to one AES-256 key"));
        Self {
            key: LessSafeKey::new(UnboundKey::from(okm)),
            rng: SystemRandom::new(),
        }
    }

    /// `FORMAT_V1 || nonce || ciphertext || tag`, bound to `context`.
    pub fn seal(&self, plaintext: &str, context: &[u8]) -> Vec<u8> {
        let mut nonce = [0_u8; NONCE_LEN];
        self.rng
            .fill(&mut nonce)
            .unwrap_or_else(|_| unreachable!("the system random number generator failed"));
        let mut sealed = Vec::with_capacity(1 + NONCE_LEN + plaintext.len() + 16);
        sealed.push(FORMAT_V1);
        sealed.extend_from_slice(&nonce);
        let mut body = plaintext.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(context),
                &mut body,
            )
            .unwrap_or_else(|_| unreachable!("AES-GCM seals any value shorter than 64 GiB"));
        sealed.extend_from_slice(&body);
        sealed
    }

    /// The plaintext of `sealed`, which must have been sealed for `context`.
    pub fn open(&self, sealed: &[u8], context: &[u8]) -> Result<String, OpenError> {
        let (&format, rest) = sealed.split_first().ok_or(OpenError)?;
        if format != FORMAT_V1 || rest.len() < NONCE_LEN {
            return Err(OpenError);
        }
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| OpenError)?;
        let mut body = body.to_vec();
        let plaintext = self
            .key
            .open_in_place(nonce, Aad::from(context), &mut body)
            .map_err(|_| OpenError)?;
        String::from_utf8(plaintext.to_vec()).map_err(|_| OpenError)
    }

    /// [`seal`](Self::seal), as URL-safe base64 (for a cookie).
    pub fn seal_text(&self, plaintext: &str, context: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(self.seal(plaintext, context))
    }

    /// [`open`](Self::open) of a [`seal_text`](Self::seal_text) value.
    pub fn open_text(&self, sealed: &str, context: &[u8]) -> Result<String, OpenError> {
        let bytes = URL_SAFE_NO_PAD.decode(sealed).map_err(|_| OpenError)?;
        self.open(&bytes, context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher(byte: u8, purpose: &'static [u8]) -> Cipher {
        Cipher::new(&SecretKey::from_bytes([byte; SECRET_KEY_LEN]), purpose)
    }

    #[test]
    fn sealed_values_open_only_with_their_key_purpose_and_context() {
        let a = cipher(1, b"test v1");
        let sealed = a.seal("refresh-token", b"row-1");
        assert_eq!(a.open(&sealed, b"row-1").unwrap(), "refresh-token");
        assert_eq!(a.open(&sealed, b"row-2"), Err(OpenError));
        assert_eq!(
            cipher(2, b"test v1").open(&sealed, b"row-1"),
            Err(OpenError)
        );
        assert_eq!(
            cipher(1, b"other v1").open(&sealed, b"row-1"),
            Err(OpenError)
        );
        let mut tampered = sealed.clone();
        if let Some(last) = tampered.last_mut() {
            *last ^= 1;
        }
        assert_eq!(a.open(&tampered, b"row-1"), Err(OpenError));
        // A fresh nonce each time.
        assert_ne!(a.seal("x", b"c"), a.seal("x", b"c"));
    }

    #[test]
    fn text_round_trips() {
        let a = cipher(3, b"test v1");
        let sealed = a.seal_text("hello", b"ctx");
        assert!(
            sealed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(a.open_text(&sealed, b"ctx").unwrap(), "hello");
        assert_eq!(a.open_text("not base64!", b"ctx"), Err(OpenError));
    }

    #[test]
    fn keys_decode_and_never_print() {
        let key = SecretKey::decode(&STANDARD.encode([7_u8; 32])).unwrap();
        assert_eq!(format!("{key:?}"), "SecretKey(<redacted>)");
        assert!(SecretKey::decode("c2hvcnQ=").unwrap_err().contains("32"));
        assert!(SecretKey::decode("%%%").is_err());
    }

    #[test]
    fn derived_keys_depend_on_the_secret_and_the_purpose() {
        let a = SecretKey::from_bytes([1; SECRET_KEY_LEN]);
        let b = SecretKey::from_bytes([2; SECRET_KEY_LEN]);
        assert_eq!(a.derive(b"test v1"), a.derive(b"test v1"));
        assert_ne!(a.derive(b"test v1"), a.derive(b"other v1"));
        assert_ne!(a.derive(b"test v1"), b.derive(b"test v1"));
    }

    #[test]
    fn tokens_hashes_and_pkce() {
        let token = random_token();
        assert_eq!(token.len(), 43);
        assert_ne!(token, random_token());
        assert!(same_hash(&hash("a"), &hash("a")));
        assert!(!same_hash(&hash("a"), &hash("b")));
        // RFC 7636 appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
