//! The `/bff/*` endpoints.

use applicamp_identity::{Provider, ProviderIdentity, SignIn};
use applicamp_kernel::{
    Error, Problem,
    crypto::{self, pkce_challenge, random_token},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{Bff, LOGIN_CONTEXT, LOGIN_LIFETIME, oidc::OidcError, store};

/// Where a failed sign-in lands.
pub const SIGN_IN_FAILED: &str = "/sign-in?error=sign_in_failed";
/// Where a provider that cannot be reached sends the browser.
pub const PROVIDER_UNAVAILABLE: &str = "/sign-in?error=provider_unavailable";
/// Where someone not on the invite list lands.
pub const NOT_INVITED: &str = "/not-invited";

/// The sign-in in progress, sealed into the login cookie.
#[derive(Serialize, Deserialize)]
struct LoginState {
    provider: Provider,
    state: String,
    nonce: String,
    verifier: String,
    /// Unix seconds.
    expires_at: i64,
}

fn redirect(to: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(location) = HeaderValue::from_str(to) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

fn unknown_provider() -> Response {
    Error::NotFound.into_response()
}

// --- providers -------------------------------------------------------------------

/// The sign-in providers this instance offers.
#[derive(Debug, Serialize, ToSchema)]
pub struct Providers {
    /// In the order the sign-in page shows them.
    pub providers: Vec<Provider>,
}

/// The sign-in providers.
///
/// Only the configured ones; the sign-in page shows one button each.
#[utoipa::path(
    get,
    path = "/bff/providers",
    tag = "bff",
    operation_id = "bff.providers",
    security(()),
    responses((status = 200, description = "The configured providers.", body = Providers)),
)]
pub async fn providers(State(bff): State<Bff>) -> Json<Providers> {
    Json(Providers {
        providers: bff.inner.providers.iter().map(|p| p.provider()).collect(),
    })
}

// --- sign-in ---------------------------------------------------------------------

/// Start signing in with a provider.
///
/// Seals a new sign-in (`state`, `nonce`, PKCE verifier) into a
/// ten-minute, `SameSite=Lax` login cookie and redirects to the provider's
/// authorization endpoint.
#[utoipa::path(
    get,
    path = "/bff/sign-in/{provider}",
    tag = "bff",
    operation_id = "bff.sign_in",
    params(("provider" = Provider, Path, description = "`campbooks` or `linkedin`.")),
    security(()),
    responses(
        (status = 303, description = "To the provider, or to \
            `/sign-in?error=provider_unavailable` when it cannot be reached.",
            headers(("location" = String), ("set-cookie" = String))),
        (status = 404, description = "`not_found`: no such provider is configured.",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn sign_in(State(bff): State<Bff>, Path(provider): Path<String>) -> Response {
    let Some(oidc) = Provider::parse(&provider).and_then(|p| bff.provider(p)) else {
        return unknown_provider();
    };
    let login = LoginState {
        provider: oidc.provider(),
        state: random_token(),
        nonce: random_token(),
        verifier: random_token(),
        expires_at: (bff.now() + LOGIN_LIFETIME).timestamp(),
    };
    let url = match oidc
        .authorization_url(&login.state, &login.nonce, &pkce_challenge(&login.verifier))
        .await
    {
        Ok(url) => url,
        Err(err) => {
            tracing::warn!(provider = %oidc.provider(), error = %err, "sign-in could not start");
            return redirect(PROVIDER_UNAVAILABLE);
        }
    };
    let Ok(json) = serde_json::to_string(&login) else {
        return Error::internal("the sign-in state does not serialize").into_response();
    };
    let sealed = bff.inner.login.seal_text(&json, LOGIN_CONTEXT);
    with_cookie(
        redirect(&url),
        bff.inner
            .cookies
            .set_login(&sealed, LOGIN_LIFETIME.num_seconds()),
    )
}

// --- callback --------------------------------------------------------------------

/// What the provider sends back.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CallbackQuery {
    /// The authorization code.
    code: Option<String>,
    /// The `state` sent with the authorization request.
    state: Option<String>,
    /// An error code instead of a code.
    error: Option<String>,
}

/// Why a callback failed: logged by kind, never with its values.
enum Failure {
    /// No login cookie, another provider's, an expired one, or a `state`
    /// that does not match it.
    State,
    /// The provider answered with an error.
    Provider,
    /// No code.
    NoCode,
    /// The code exchange or the ID token failed.
    Oidc(OidcError),
    /// The database failed.
    Database,
}

impl Failure {
    fn kind(&self) -> &'static str {
        match self {
            Self::State => "invalid_state",
            Self::Provider => "provider_error",
            Self::NoCode => "no_code",
            Self::Oidc(err) => err.kind(),
            Self::Database => "database",
        }
    }
}

/// Finish signing in.
///
/// Checks `state` against the login cookie, redeems the code, validates the
/// ID token, and signs the person in if their provider identity is linked
/// or their verified address is on the invite list: a session cookie and a
/// redirect to `/`. Someone not on the list lands on `/not-invited`; any
/// failure on `/sign-in?error=sign_in_failed`. The login cookie is cleared
/// either way.
#[utoipa::path(
    get,
    path = "/bff/callback/{provider}",
    tag = "bff",
    operation_id = "bff.callback",
    params(
        ("provider" = Provider, Path, description = "`campbooks` or `linkedin`."),
        CallbackQuery,
    ),
    security(()),
    responses(
        (status = 303, description = "To `/` signed in (with the session cookie), to \
            `/not-invited`, or to `/sign-in?error=sign_in_failed`.",
            headers(("location" = String), ("set-cookie" = String))),
        (status = 404, description = "`not_found`: no such provider is configured.",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn callback(
    State(bff): State<Bff>,
    Path(provider): Path<String>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(provider) = Provider::parse(&provider).filter(|p| bff.provider(*p).is_some()) else {
        return unknown_provider();
    };
    let response = match finish(&bff, provider, query, &headers).await {
        Ok(response) => response,
        Err(failure) => {
            tracing::warn!(%provider, reason = failure.kind(), "sign-in failed");
            redirect(SIGN_IN_FAILED)
        }
    };
    with_cookie(response, bff.inner.cookies.clear_login())
}

async fn finish(
    bff: &Bff,
    provider: Provider,
    query: CallbackQuery,
    headers: &HeaderMap,
) -> Result<Response, Failure> {
    let login = bff
        .inner
        .cookies
        .login(headers)
        .and_then(|sealed| bff.inner.login.open_text(&sealed, LOGIN_CONTEXT).ok())
        .and_then(|json| serde_json::from_str::<LoginState>(&json).ok())
        .ok_or(Failure::State)?;
    let state_matches = query
        .state
        .as_deref()
        .is_some_and(|state| crypto::same_hash(state.as_bytes(), login.state.as_bytes()));
    if login.provider != provider || !state_matches || login.expires_at <= bff.now().timestamp() {
        return Err(Failure::State);
    }
    if query.error.is_some() {
        return Err(Failure::Provider);
    }
    let code = query.code.ok_or(Failure::NoCode)?;
    let oidc = bff.provider(provider).ok_or(Failure::State)?;
    let verified = oidc
        .sign_in(&code, &login.verifier, &login.nonce)
        .await
        .map_err(Failure::Oidc)?;

    let who = ProviderIdentity {
        provider,
        subject: verified.subject,
        email: verified.email,
        email_verified: verified.email_verified,
        name: verified.name,
        picture_url: verified.picture,
        refresh_token: verified.refresh_token,
    };
    let account = match applicamp_identity::sign_in(&bff.inner.state, who).await {
        Ok(SignIn::Account(account)) => account,
        Ok(SignIn::NotInvited) => return Ok(redirect(NOT_INVITED)),
        Err(err) => {
            tracing::error!(code = err.code(), "sign-in could not be recorded");
            return Err(Failure::Database);
        }
    };
    let (_, token) = store::create(&bff.inner.state.db, account, bff.now())
        .await
        .map_err(|_| Failure::Database)?;
    tracing::info!(%provider, "signed in");
    Ok(with_cookie(
        redirect("/"),
        bff.inner
            .cookies
            .set_session(&token, store::LIFETIME.num_seconds()),
    ))
}

// --- sign-out --------------------------------------------------------------------

/// Sign out.
///
/// Deletes the session on the server and clears the cookie. Needs the
/// `X-Applicamp-CSRF: 1` header and a same-origin request, like every
/// unsafe request.
#[utoipa::path(
    post,
    path = "/bff/sign-out",
    tag = "bff",
    operation_id = "bff.sign_out",
    security(("session" = [])),
    responses(
        (status = 204, description = "Signed out (also when there was no session).",
            headers(("set-cookie" = String))),
        (status = 403, description = "`csrf_check_failed`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn sign_out(State(bff): State<Bff>, headers: HeaderMap) -> Response {
    if let Some(token) = bff.inner.cookies.session(&headers)
        && let Err(err) = store::delete(&bff.inner.state.db, &token).await
    {
        return Error::from(err).into_response();
    }
    with_cookie(
        StatusCode::NO_CONTENT.into_response(),
        bff.inner.cookies.clear_session(),
    )
}
