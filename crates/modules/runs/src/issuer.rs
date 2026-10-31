//! The OpenShell gateway trusts this instance as its OIDC issuer (v0.1.6
//! makes gateway auth mandatory).
//!
//! The server mints itself five-minute JWTs for its own gRPC calls to the
//! gateway and publishes the key that checks them; nothing else is ever
//! issued. The key is Ed25519, derived from the instance secret for this
//! purpose only, with a stable `kid`. The gateway is configured with
//!
//! ```toml
//! [openshell.gateway.oidc]
//! issuer = "<APPLICAMP_PUBLIC_URL's origin>"   # exactly, no trailing slash
//! audience = "openshell-gateway"
//! roles_claim = "roles"
//! ```
//!
//! and checks `iss`, `aud`, `exp` and `sub` on every call, finding the key
//! by `kid` in the JWKS at the issuer's own origin.

use std::time::Duration;

use applicamp_kernel::crypto::SecretKey;
use aws_lc_rs::{
    digest,
    signature::{Ed25519KeyPair, KeyPair as _},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::{Value, json};

/// The `aud` the gateway expects.
pub const AUDIENCE: &str = "openshell-gateway";
/// The `sub` of every token: this server, and nobody else.
pub const SUBJECT: &str = "applicamp-server";
/// The gateway role the server acts with (`admin_role`, its default).
pub const ADMIN_ROLE: &str = "openshell-admin";
/// How long a token lasts.
pub const LIFETIME: Duration = Duration::from_secs(5 * 60);
/// Where the keys are published, under the issuer.
pub const JWKS_PATH: &str = "/oauth2/jwks";
/// The key's purpose label. Never change it: the gateway caches the key.
const PURPOSE: &[u8] = b"applicamp gateway oidc v1";

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    sub: &'a str,
    iat: u64,
    nbf: u64,
    exp: u64,
    roles: [&'a str; 1],
}

/// The issuer. Holds the signing key; never printed.
pub struct Issuer {
    issuer: String,
    kid: String,
    x: String,
    key: EncodingKey,
}

impl std::fmt::Debug for Issuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Issuer")
            .field("issuer", &self.issuer)
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}

impl Issuer {
    /// The issuer at `origin` (`https://applicamp.example.com`), keyed from
    /// `secret`.
    pub fn new(secret: &SecretKey, origin: &str) -> Result<Self, String> {
        let seed = secret.derive(PURPOSE);
        let pair = Ed25519KeyPair::from_seed_unchecked(&seed)
            .map_err(|_| "the gateway key could not be derived".to_owned())?;
        let pkcs8 = pair
            .to_pkcs8v1()
            .map_err(|_| "the gateway key could not be encoded".to_owned())?;
        let public = pair.public_key().as_ref().to_vec();
        let thumb = digest::digest(&digest::SHA256, &public);
        Ok(Self {
            issuer: origin.trim_end_matches('/').to_owned(),
            kid: URL_SAFE_NO_PAD.encode(&thumb.as_ref()[..12]),
            x: URL_SAFE_NO_PAD.encode(&public),
            key: EncodingKey::from_ed_der(pkcs8.as_ref()),
        })
    }

    /// The issuer, as `iss` and the discovery document say it.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The key's id.
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// `/.well-known/openid-configuration`.
    pub fn discovery(&self) -> Value {
        json!({
            "issuer": self.issuer,
            "jwks_uri": format!("{}{JWKS_PATH}", self.issuer),
            "response_types_supported": ["token"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["EdDSA"],
        })
    }

    /// The key set: the one public key.
    pub fn jwks(&self) -> Value {
        json!({ "keys": [{
            "kty": "OKP",
            "crv": "Ed25519",
            "x": self.x,
            "kid": self.kid,
            "alg": "EdDSA",
            "use": "sig",
        }]})
    }

    /// A token for the gateway, issued at `now` (Unix seconds), and when it
    /// expires.
    pub fn mint(&self, now: u64) -> Result<(String, u64), String> {
        let exp = now + LIFETIME.as_secs();
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        let claims = Claims {
            iss: &self.issuer,
            aud: AUDIENCE,
            sub: SUBJECT,
            iat: now,
            nbf: now,
            exp,
            roles: [ADMIN_ROLE],
        };
        let token = jsonwebtoken::encode(&header, &claims, &self.key)
            .map_err(|e| format!("a gateway token could not be signed: {e}"))?;
        Ok((token, exp))
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};

    use super::*;

    const ORIGIN: &str = "https://applicamp.example.com";

    fn issuer(byte: u8) -> Issuer {
        Issuer::new(&SecretKey::from_bytes([byte; 32]), ORIGIN).unwrap()
    }

    fn now() -> u64 {
        u64::try_from(chrono::Utc::now().timestamp()).unwrap()
    }

    /// The gateway's check (openshell-server `auth/oidc.rs`): the key from
    /// the JWKS by `kid`, EdDSA, `iss` and `aud` exactly, and `iss`, `aud`,
    /// `exp`, `sub` required.
    fn gateway_accepts(jwks: &Value, token: &str) -> Result<Value, String> {
        let kid = decode_header(token).map_err(|e| e.to_string())?.kid;
        let key = jwks["keys"]
            .as_array()
            .and_then(|keys| keys.iter().find(|k| k["kid"].as_str() == kid.as_deref()))
            .ok_or("no key with that kid")?;
        assert_eq!(
            (key["kty"].as_str(), key["crv"].as_str()),
            (Some("OKP"), Some("Ed25519"))
        );
        let decoding = DecodingKey::from_ed_components(key["x"].as_str().unwrap_or_default())
            .map_err(|e| e.to_string())?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[ORIGIN]);
        validation.set_audience(&[AUDIENCE]);
        validation.set_required_spec_claims(&["iss", "aud", "exp", "sub"]);
        decode::<Value>(token, &decoding, &validation)
            .map(|data| data.claims)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn the_gateway_accepts_its_tokens() {
        let issuer = issuer(1);
        let (token, exp) = issuer.mint(now()).unwrap();
        let claims = gateway_accepts(&issuer.jwks(), &token).unwrap();
        assert_eq!(claims["sub"], SUBJECT);
        assert_eq!(claims["roles"], json!([ADMIN_ROLE]));
        assert_eq!(claims["exp"], exp);
        assert_eq!(issuer.discovery()["issuer"], ORIGIN);
        assert_eq!(
            issuer.discovery()["jwks_uri"],
            format!("{ORIGIN}/oauth2/jwks")
        );
    }

    #[test]
    fn the_key_is_stable_and_its_own() {
        assert_eq!(issuer(1).kid(), issuer(1).kid());
        assert_eq!(issuer(1).jwks(), issuer(1).jwks());
        assert_ne!(issuer(1).kid(), issuer(2).kid());
        let (token, _) = issuer(2).mint(now()).unwrap();
        assert!(gateway_accepts(&issuer(1).jwks(), &token).is_err());
        let trailing = Issuer::new(
            &SecretKey::from_bytes([1; 32]),
            "https://applicamp.example.com/",
        )
        .unwrap();
        assert_eq!(trailing.issuer(), ORIGIN);
    }

    #[test]
    fn expired_tokens_are_refused() {
        let issuer = issuer(1);
        let (token, _) = issuer.mint(now() - 3600).unwrap();
        assert!(gateway_accepts(&issuer.jwks(), &token).is_err());
        assert!(!format!("{issuer:?}").contains(&issuer.x));
    }
}
