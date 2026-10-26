//! The web backend-for-frontend: the browser never holds a token.
//!
//! The server runs the OpenID Connect authorization-code flow (PKCE S256)
//! as a confidential client of each configured provider, decides with the
//! identity module who signed in, and gives the browser one opaque,
//! `HttpOnly`, `SameSite=Strict` session cookie, whose hash it keeps in
//! Postgres (schema `bff`).
//!
//! | Endpoint | Does |
//! | --- | --- |
//! | `GET /bff/providers` | the configured providers |
//! | `GET /bff/sign-in/{provider}` | starts a sign-in, redirects to the provider |
//! | `GET /bff/callback/{provider}` | finishes it: code exchange, ID token check, session |
//! | `POST /bff/sign-out` | ends the session |
//!
//! [`authenticate`] resolves the session cookie for the API routes; unsafe
//! requests everywhere pass the [`csrf`] check.

mod cookies;
pub mod csrf;
mod handlers;
mod oidc;
pub mod store;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use applicamp_identity::Provider;
use applicamp_kernel::{
    AppState,
    auth::Principal,
    config::{Config, ConfigError, ProviderConfig, read_secret},
    crypto::Cipher,
};
use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, TimeDelta, Utc};
use secrecy::SecretString;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa_axum::{router::OpenApiRouter, routes};

pub use cookies::{LOGIN_COOKIE, SESSION_COOKIE};
pub use csrf::CSRF_HEADER;

use self::{cookies::Cookies, oidc::Oidc};

/// Name of the OpenAPI security scheme of cookie-authenticated operations:
/// `security(("session" = []))`.
pub const SESSION_SCHEME: &str = "session";

/// LinkedIn's issuer, unless `APPLICAMP_OIDC__LINKEDIN__ISSUER` says
/// otherwise (tests point it at a fake).
pub const LINKEDIN_ISSUER: &str = "https://www.linkedin.com/oauth";

/// How long a started sign-in may take at the provider.
pub const LOGIN_LIFETIME: TimeDelta = TimeDelta::minutes(10);

/// The key purpose of the login cookie. Never change it.
const LOGIN_PURPOSE: &[u8] = b"applicamp bff login v1";

/// Associated data of the sealed login cookie.
const LOGIN_CONTEXT: &[u8] = b"bff.login";

/// One configured provider.
#[derive(Debug, Clone)]
pub struct ProviderSettings {
    /// Which.
    pub provider: Provider,
    /// Its issuer (discovery lives under it).
    pub issuer: String,
    /// This instance's client id there.
    pub client_id: String,
    /// And its secret.
    pub client_secret: SecretString,
}

impl ProviderSettings {
    /// The scopes a sign-in asks for. Campbooks' mail and calendar scopes
    /// come with the correspondence change.
    pub fn provider_scopes(&self) -> &'static str {
        match self.provider {
            Provider::Campbooks => "openid profile email offline_access",
            Provider::Linkedin => "openid profile email",
        }
    }
}

/// The BFF's settings, checked once at startup.
#[derive(Debug, Clone)]
pub struct Settings {
    /// `APPLICAMP_PUBLIC_URL`'s origin, e.g. `https://applicamp.example.com`.
    pub public_origin: String,
    /// Whether that is `https` (secure cookies).
    pub secure: bool,
    /// The configured providers, in display order.
    pub providers: Vec<ProviderSettings>,
}

impl Settings {
    /// The settings `config` describes. A provider with a client id needs a
    /// secret (and Campbooks an issuer); one without is not offered.
    pub fn from_config(config: &Config) -> Result<Self, ConfigError> {
        let url = url::Url::parse(&config.public_url)
            .map_err(|e| ConfigError(format!("APPLICAMP_PUBLIC_URL={}: {e}", config.public_url)))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ConfigError(format!(
                "APPLICAMP_PUBLIC_URL={} must be an http(s) URL",
                config.public_url
            )));
        }
        let mut providers = Vec::new();
        for (provider, settings, default_issuer) in [
            (Provider::Campbooks, &config.oidc.campbooks, None),
            (
                Provider::Linkedin,
                &config.oidc.linkedin,
                Some(LINKEDIN_ISSUER),
            ),
        ] {
            if let Some(configured) = provider_settings(provider, settings, default_issuer)? {
                providers.push(configured);
            }
        }
        Ok(Self {
            public_origin: url.origin().ascii_serialization(),
            secure: url.scheme() == "https",
            providers,
        })
    }
}

fn provider_settings(
    provider: Provider,
    settings: &ProviderConfig,
    default_issuer: Option<&str>,
) -> Result<Option<ProviderSettings>, ConfigError> {
    let var = format!("APPLICAMP_OIDC__{}", provider.as_str().to_ascii_uppercase());
    let Some(client_id) = settings
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(None);
    };
    let issuer = settings
        .issuer
        .as_deref()
        .or(default_issuer)
        .ok_or_else(|| ConfigError(format!("{var}__ISSUER is required with {var}__CLIENT_ID")))?;
    if url::Url::parse(issuer).is_err() {
        return Err(ConfigError(format!("{var}__ISSUER={issuer} is not a URL")));
    }
    let client_secret = read_secret(
        &format!("{var}__CLIENT_SECRET"),
        settings.client_secret.as_ref(),
        settings.client_secret_file.as_deref(),
    )?
    .ok_or_else(|| {
        ConfigError(format!(
            "{var}__CLIENT_SECRET_FILE is required with {var}__CLIENT_ID"
        ))
    })?;
    Ok(Some(ProviderSettings {
        provider,
        issuer: issuer.to_owned(),
        client_id: client_id.to_owned(),
        client_secret,
    }))
}

/// The BFF's state. Cheap to clone.
#[derive(Clone)]
pub struct Bff {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Bff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bff")
            .field("providers", &self.inner.providers)
            .finish_non_exhaustive()
    }
}

struct Inner {
    state: AppState,
    cookies: Cookies,
    public_origin: Arc<str>,
    login: Cipher,
    providers: Vec<Oidc>,
}

impl Bff {
    /// The BFF over `state` with `settings`.
    pub fn new(state: &AppState, settings: &Settings) -> Result<Self, String> {
        let providers = settings
            .providers
            .iter()
            .map(|provider| Oidc::new(provider, &settings.public_origin))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            inner: Arc::new(Inner {
                login: Cipher::new(&state.secret, LOGIN_PURPOSE),
                state: state.clone(),
                cookies: Cookies::new(settings.secure),
                public_origin: settings.public_origin.as_str().into(),
                providers,
            }),
        })
    }

    /// The origin unsafe requests must come from.
    pub fn public_origin(&self) -> Arc<str> {
        Arc::clone(&self.inner.public_origin)
    }

    /// The configured providers.
    pub fn providers(&self) -> Vec<Provider> {
        self.inner.providers.iter().map(Oidc::provider).collect()
    }

    fn provider(&self, provider: Provider) -> Option<&Oidc> {
        self.inner
            .providers
            .iter()
            .find(|p| p.provider() == provider)
    }

    fn now(&self) -> DateTime<Utc> {
        self.inner.state.clock.now()
    }
}

/// The `/bff/*` routes, with their OpenAPI docs.
pub fn router() -> OpenApiRouter<Bff> {
    OpenApiRouter::new()
        .routes(routes!(handlers::providers))
        .routes(routes!(handlers::sign_in))
        .routes(routes!(handlers::callback))
        .routes(routes!(handlers::sign_out))
}

/// The `session` security scheme of the OpenAPI document.
pub fn security_scheme() -> SecurityScheme {
    SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
        SESSION_COOKIE,
        "The web app's session cookie, set by `/bff/callback/{provider}` (`HttpOnly`, \
         `Secure`, `SameSite=Strict`; named `applicamp_session` when the instance's public URL \
         is plain http). Unsafe methods also need the `X-Applicamp-CSRF: 1` header and the \
         app's own `Origin`.",
    )))
}

/// Middleware: when the request carries a live session cookie, leave its
/// [`Principal`] in the request's extensions for
/// [`SignedIn`](applicamp_kernel::auth::SignedIn). A session used after
/// [`store::TOUCH_EVERY`] lasts another [`store::LIFETIME`], and its cookie
/// is sent again with the new lifetime.
pub async fn authenticate(State(bff): State<Bff>, mut request: Request, next: Next) -> Response {
    let Some(token) = bff.inner.cookies.session(request.headers()) else {
        return next.run(request).await;
    };
    let now = bff.now();
    let session = match store::find(&bff.inner.state.db, &token, now).await {
        Ok(session) => session,
        Err(err) => {
            tracing::warn!(error = %err, "could not look up a session");
            None
        }
    };
    let Some(session) = session else {
        return next.run(request).await;
    };
    request.extensions_mut().insert(Principal {
        account_id: session.account_id,
        session_id: session.id,
    });
    let renewed = now - session.last_seen_at >= store::TOUCH_EVERY
        && store::touch(&bff.inner.state.db, session.id, now)
            .await
            .is_ok();
    let mut response = next.run(request).await;
    if renewed {
        response.headers_mut().append(
            header::SET_COOKIE,
            bff.inner
                .cookies
                .set_session(&token, store::LIFETIME.num_seconds()),
        );
    }
    response
}
