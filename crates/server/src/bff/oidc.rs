//! The OpenID Connect client this instance is registered as at each
//! provider (Campbooks, LinkedIn): discovery, the authorization request, the
//! code exchange (`client_secret_post`) and ID-token validation.
//!
//! The ID token comes straight from the provider's token endpoint over TLS,
//! which OIDC allows to trust without a signature check; it is checked
//! anyway (signature against the provider's published keys, issuer,
//! audience, `azp`, expiry, nonce), so a misdirected or forged response
//! cannot sign anyone in.
//!
//! Errors never carry tokens, codes, secrets or addresses; only the
//! provider's `error` code or the kind of failure.

use std::{sync::Arc, time::Duration};

use applicamp_identity::Provider;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation,
    jwk::{AlgorithmParameters, JwkSet},
};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use tokio::sync::Mutex;

use super::ProviderSettings;

/// Timeout of one request to a provider.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
/// Largest response read from a provider.
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
/// Clock skew tolerated on `exp`, `nbf` and `iat`.
const LEEWAY_SECONDS: u64 = 60;
/// Signature algorithms accepted on ID tokens; everything else (`none`,
/// HMAC, …) is refused before any key is looked up.
const ALLOWED_ALGORITHMS: [Algorithm; 2] = [Algorithm::EdDSA, Algorithm::RS256];

/// Why a call to a provider failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OidcError {
    /// The provider refused the request (`invalid_grant`, …).
    #[error("the provider refused the request: {0}")]
    Rejected(String),
    /// The provider could not be reached or answered something unexpected.
    #[error("the provider is unavailable: {0}")]
    Unavailable(String),
    /// The ID token did not validate; which check failed.
    #[error("the ID token is invalid: {0}")]
    InvalidIdToken(&'static str),
}

impl OidcError {
    /// The kind logged and recorded.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Rejected(_) => "code_rejected",
            Self::Unavailable(_) => "provider_unavailable",
            Self::InvalidIdToken(_) => "invalid_id_token",
        }
    }
}

/// Who signed in, as the validated ID token says.
pub struct Verified {
    /// `sub`.
    pub subject: String,
    /// `email`.
    pub email: Option<String>,
    /// `email_verified` (`true` or `"true"`).
    pub email_verified: bool,
    /// `name`.
    pub name: Option<String>,
    /// `picture`.
    pub picture: Option<String>,
    /// The refresh token, when the provider granted one.
    pub refresh_token: Option<SecretString>,
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<String>,
}

/// The ID token claims read, beyond the ones `jsonwebtoken` checks.
#[derive(Deserialize)]
struct Claims {
    sub: String,
    nonce: Option<String>,
    azp: Option<String>,
    email: Option<String>,
    email_verified: Option<serde_json::Value>,
    name: Option<String>,
    picture: Option<String>,
}

/// The client of one provider.
pub struct Oidc {
    provider: Provider,
    http: reqwest::Client,
    issuer: String,
    client_id: String,
    client_secret: SecretString,
    redirect_uri: String,
    scopes: &'static str,
    discovery: Mutex<Option<Arc<Discovery>>>,
}

impl std::fmt::Debug for Oidc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oidc")
            .field("provider", &self.provider)
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

impl Oidc {
    /// The client for `settings`, coming back to `public_origin`.
    pub fn new(settings: &ProviderSettings, public_origin: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("applicamp/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("could not build the {} client: {e}", settings.provider))?;
        Ok(Self {
            provider: settings.provider,
            http,
            issuer: settings.issuer.trim_end_matches('/').to_owned(),
            client_id: settings.client_id.clone(),
            client_secret: settings.client_secret.clone(),
            redirect_uri: format!("{public_origin}/bff/callback/{}", settings.provider),
            scopes: settings.provider_scopes(),
            discovery: Mutex::default(),
        })
    }

    /// The provider.
    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// The URL to send the browser to.
    pub async fn authorization_url(
        &self,
        state: &str,
        nonce: &str,
        code_challenge: &str,
    ) -> Result<String, OidcError> {
        let discovery = self.discovery().await?;
        let mut url = url::Url::parse(&discovery.authorization_endpoint)
            .map_err(|_| OidcError::Unavailable("a bad authorization endpoint".into()))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", self.scopes)
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    /// Redeem `code` (with the PKCE `verifier`) and validate the ID token
    /// for the round trip that sent `nonce`.
    pub async fn sign_in(
        &self,
        code: &str,
        verifier: &str,
        nonce: &str,
    ) -> Result<Verified, OidcError> {
        let discovery = self.discovery().await?;
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("code_verifier", verifier)
            .append_pair("client_id", &self.client_id)
            .append_pair("client_secret", self.client_secret.expose_secret())
            .finish();
        let response = self
            .http
            .post(&discovery.token_endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form)
            .send()
            .await
            .map_err(|e| OidcError::Unavailable(e.without_url().to_string()))?;
        let status = response.status();
        let body = read_body(response).await?;
        if !status.is_success() {
            let error = serde_json::from_slice::<ErrorBody>(&body)
                .ok()
                .and_then(|body| body.error)
                .filter(|error| error.len() <= 64 && error.bytes().all(|b| b.is_ascii_graphic()))
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(if status.is_client_error() {
                OidcError::Rejected(error)
            } else {
                OidcError::Unavailable(error)
            });
        }
        let tokens: TokenResponse = serde_json::from_slice(&body)
            .map_err(|_| OidcError::Unavailable("an unexpected token response".into()))?;
        let id_token = tokens
            .id_token
            .ok_or(OidcError::InvalidIdToken("no ID token"))?;
        let claims = self.validate(&discovery, &id_token, nonce).await?;
        Ok(Verified {
            subject: claims.sub,
            email: claims.email,
            email_verified: is_true(claims.email_verified.as_ref()),
            name: claims.name,
            picture: claims.picture,
            refresh_token: tokens.refresh_token.map(SecretString::from),
        })
    }

    async fn validate(
        &self,
        discovery: &Discovery,
        token: &str,
        nonce: &str,
    ) -> Result<Claims, OidcError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|_| OidcError::InvalidIdToken("malformed"))?;
        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            return Err(OidcError::InvalidIdToken("algorithm"));
        }
        let kid = header
            .kid
            .as_deref()
            .ok_or(OidcError::InvalidIdToken("no key id"))?;
        let key = self
            .key(discovery, kid, header.alg)
            .await?
            .ok_or(OidcError::InvalidIdToken("unknown key"))?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&discovery.issuer]);
        validation.set_audience(&[&self.client_id]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_nbf = true;
        validation.leeway = LEEWAY_SECONDS;
        let claims = jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .map_err(|_| OidcError::InvalidIdToken("signature, issuer, audience or expiry"))?
            .claims;

        if claims
            .azp
            .as_deref()
            .is_some_and(|azp| azp != self.client_id)
        {
            return Err(OidcError::InvalidIdToken("authorized party"));
        }
        let nonce_matches = claims.nonce.as_deref().is_some_and(|sent| {
            applicamp_kernel::crypto::same_hash(sent.as_bytes(), nonce.as_bytes())
        });
        if !nonce_matches {
            return Err(OidcError::InvalidIdToken("nonce"));
        }
        Ok(claims)
    }

    /// The provider's key `kid` for `alg`, from its JWKS (fetched each
    /// time: sign-ins are rare, and a rotated key is then never missed).
    async fn key(
        &self,
        discovery: &Discovery,
        kid: &str,
        alg: Algorithm,
    ) -> Result<Option<DecodingKey>, OidcError> {
        let body = self.get(&discovery.jwks_uri, "keys").await?;
        let keys: JwkSet = serde_json::from_slice(&body)
            .map_err(|_| OidcError::Unavailable("an unexpected key set".into()))?;
        let Some(jwk) = keys.find(kid) else {
            return Ok(None);
        };
        Ok(match (&jwk.algorithm, alg) {
            // Some providers publish `n` with a leading zero byte, which the
            // RSA backend refuses when it verifies; the number is the same
            // without it.
            (AlgorithmParameters::RSA(params), Algorithm::RS256) => {
                let n = URL_SAFE_NO_PAD.decode(&params.n).ok();
                let e = URL_SAFE_NO_PAD.decode(&params.e).ok();
                n.zip(e).map(|(n, e)| {
                    DecodingKey::from_rsa_raw_components(
                        without_leading_zeros(&n),
                        without_leading_zeros(&e),
                    )
                })
            }
            (AlgorithmParameters::OctetKeyPair(_), Algorithm::EdDSA) => {
                DecodingKey::from_jwk(jwk).ok()
            }
            _ => None,
        })
    }

    async fn discovery(&self) -> Result<Arc<Discovery>, OidcError> {
        let mut cached = self.discovery.lock().await;
        if let Some(discovery) = cached.as_ref() {
            return Ok(Arc::clone(discovery));
        }
        let url = format!("{}/.well-known/openid-configuration", self.issuer);
        let body = self.get(&url, "discovery").await?;
        let discovery: Discovery = serde_json::from_slice(&body)
            .map_err(|_| OidcError::Unavailable("an unexpected discovery document".into()))?;
        if discovery.issuer.trim_end_matches('/') != self.issuer {
            return Err(OidcError::Unavailable(
                "discovery names another issuer".into(),
            ));
        }
        let discovery = Arc::new(discovery);
        *cached = Some(Arc::clone(&discovery));
        Ok(discovery)
    }

    async fn get(&self, url: &str, what: &str) -> Result<Vec<u8>, OidcError> {
        let response = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| OidcError::Unavailable(format!("{what}: {}", e.without_url())))?;
        if !response.status().is_success() {
            return Err(OidcError::Unavailable(format!(
                "{what}: HTTP {}",
                response.status().as_u16()
            )));
        }
        read_body(response).await
    }
}

/// `email_verified` as a boolean: some providers send the string `"true"`.
fn is_true(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(verified)) => *verified,
        Some(serde_json::Value::String(verified)) => verified == "true",
        _ => false,
    }
}

/// A big-endian integer in its minimal encoding.
fn without_leading_zeros(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    &bytes[start..]
}

async fn read_body(response: reqwest::Response) -> Result<Vec<u8>, OidcError> {
    let body = response
        .bytes()
        .await
        .map_err(|e| OidcError::Unavailable(e.without_url().to_string()))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(OidcError::Unavailable("response too large".into()));
    }
    Ok(body.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_verified_reads_booleans_and_strings() {
        assert!(is_true(Some(&serde_json::json!(true))));
        assert!(is_true(Some(&serde_json::json!("true"))));
        assert!(!is_true(Some(&serde_json::json!(false))));
        assert!(!is_true(Some(&serde_json::json!("yes"))));
        assert!(!is_true(None));
    }

    #[test]
    fn leading_zeros_are_dropped() {
        assert_eq!(without_leading_zeros(&[0, 0, 1, 0]), &[1, 0]);
        assert_eq!(without_leading_zeros(&[0]), &[] as &[u8]);
    }

    #[test]
    fn errors_name_a_kind_never_a_token() {
        let err = OidcError::InvalidIdToken("nonce");
        assert_eq!(err.kind(), "invalid_id_token");
        assert_eq!(err.to_string(), "the ID token is invalid: nonce");
    }
}
