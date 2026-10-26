//! The sign-in end to end: the app over a real Postgres (`#[sqlx::test]`)
//! and two fake OpenID Connect providers (wiremock: discovery, keys, token
//! endpoint), one for Campbooks and one for LinkedIn.

use applicamp_identity::invites;
use applicamp_kernel::{AppState, Config, config::ProviderConfig, crypto::SecretKey};
use aws_lc_rs::signature::Ed25519KeyPair;
use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use http_body_util::BodyExt as _;
use jsonwebtoken::{
    Algorithm, EncodingKey, Header,
    jwk::{Jwk, PublicKeyUse},
};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::{Bff, CSRF_HEADER, Settings, handlers};

const ORIGIN: &str = "http://localhost:8080";
const KID: &str = "test-key";

fn signing_key() -> EncodingKey {
    let pkcs8 = Ed25519KeyPair::generate()
        .and_then(|pair| pair.to_pkcs8v1())
        .unwrap();
    EncodingKey::from_ed_der(pkcs8.as_ref())
}

/// A fake OpenID Connect provider.
struct Fake {
    server: MockServer,
    key: EncodingKey,
    client_id: &'static str,
}

impl Fake {
    async fn start(client_id: &'static str) -> Self {
        let server = MockServer::start().await;
        let key = signing_key();
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::EdDSA).unwrap();
        jwk.common.key_id = Some(KID.to_owned());
        jwk.common.public_key_use = Some(PublicKeyUse::Signature);
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": uri,
                "authorization_endpoint": format!("{uri}/authorize"),
                "token_endpoint": format!("{uri}/token"),
                "jwks_uri": format!("{uri}/jwks"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "keys": [jwk] })))
            .mount(&server)
            .await;
        Self {
            server,
            key,
            client_id,
        }
    }

    fn settings(&self) -> ProviderConfig {
        ProviderConfig {
            issuer: Some(self.server.uri()),
            client_id: Some(self.client_id.to_owned()),
            client_secret: Some(SecretString::from(format!("{}-secret", self.client_id))),
            client_secret_file: None,
        }
    }

    /// Valid ID token claims for `subject` at `email`, for `nonce`.
    fn claims(&self, subject: &str, email: &str, nonce: &str) -> Value {
        let now = jsonwebtoken::get_current_timestamp();
        json!({
            "iss": self.server.uri(),
            "aud": self.client_id,
            "sub": subject,
            "email": email,
            "email_verified": true,
            "name": "Ana Example",
            "nonce": nonce,
            "iat": now,
            "exp": now + 300,
        })
    }

    fn sign(&self, claims: &Value) -> String {
        sign_with(&self.key, claims)
    }

    /// Answer the next code exchange with `id_token`.
    async fn answer(&self, id_token: String) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "provider-access-token",
                "token_type": "Bearer",
                "expires_in": 300,
                "refresh_token": "provider-refresh-token",
                "id_token": id_token,
            })))
            .up_to_n_times(1)
            .mount(&self.server)
            .await;
    }

    async fn token_requests(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| request.url.path() == "/token")
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

fn sign_with(key: &EncodingKey, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(KID.to_owned());
    jsonwebtoken::encode(&header, claims, key).unwrap()
}

struct Harness {
    app: Router,
    db: PgPool,
    campbooks: Fake,
    linkedin: Fake,
}

impl Harness {
    async fn new(db: PgPool) -> Self {
        let campbooks = Fake::start("applicamp-campbooks").await;
        let linkedin = Fake::start("applicamp-linkedin").await;
        let mut config = Config::for_database("postgres://unused");
        config.public_url = ORIGIN.to_owned();
        config.oidc.campbooks = campbooks.settings();
        config.oidc.linkedin = linkedin.settings();
        let settings = Settings::from_config(&config).unwrap();
        let state = AppState::new(db.clone(), config, SecretKey::from_bytes([9; 32]));
        let bff = Bff::new(&state, &settings).unwrap();
        Self {
            app: crate::app(state, &bff),
            db,
            campbooks,
            linkedin,
        }
    }

    fn fake(&self, provider: &str) -> &Fake {
        if provider == "campbooks" {
            &self.campbooks
        } else {
            &self.linkedin
        }
    }

    async fn send(&self, request: Request<Body>) -> Response<Body> {
        self.app.clone().oneshot(request).await.unwrap()
    }

    async fn get(&self, uri: &str, cookie: Option<&str>) -> Response<Body> {
        let mut request = Request::get(uri);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    /// Start signing in: the login cookie, and the authorization request's
    /// query.
    async fn start(&self, provider: &str) -> (String, url::Url) {
        let response = self.get(&format!("/bff/sign-in/{provider}"), None).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = url::Url::parse(location(&response)).unwrap();
        let cookie = cookie_named(&response, "applicamp_login").unwrap();
        (cookie, location)
    }

    /// Finish a sign-in the browser holding `cookie` started, with `state`.
    async fn callback(&self, provider: &str, cookie: Option<&str>, state: &str) -> Response<Body> {
        self.get(
            &format!("/bff/callback/{provider}?code=the-code&state={state}"),
            cookie,
        )
        .await
    }

    /// Sign in with `provider` as `subject` at `email`; the callback's
    /// answer.
    async fn sign_in(&self, provider: &str, subject: &str, email: &str) -> Response<Body> {
        let (cookie, authorize) = self.start(provider).await;
        let nonce = param(&authorize, "nonce");
        let fake = self.fake(provider);
        fake.answer(fake.sign(&fake.claims(subject, email, &nonce)))
            .await;
        self.callback(provider, Some(&cookie), &param(&authorize, "state"))
            .await
    }

    async fn me(&self, session: Option<&str>) -> (StatusCode, Value) {
        let response = self.get("/api/v1/me", session).await;
        let status = response.status();
        (status, body(response).await)
    }

    async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.db).await.unwrap()
    }
}

fn param(url: &url::Url, name: &str) -> String {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| panic!("no {name} in {url}"))
}

fn location(response: &Response<Body>) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

/// The `name=value` pair of a non-empty `Set-Cookie` named `name`.
fn cookie_named(response: &Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find(|pair| {
            pair.split_once('=')
                .is_some_and(|(key, value)| key == name && !value.is_empty())
        })
        .map(ToOwned::to_owned)
}

fn clears(response: &Response<Body>, name: &str) -> bool {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.starts_with(&format!("{name}=;")) && value.contains("Max-Age=0"))
}

async fn body(response: Response<Body>) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn invite(db: &PgPool, email: &str) {
    invites::add(db, email, None, chrono::Utc::now())
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invited_person_signs_in_with_campbooks_and_lands_on_today(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;

    let (cookie, authorize) = h.start("campbooks").await;
    assert_eq!(
        format!(
            "{}{}",
            authorize.origin().ascii_serialization(),
            authorize.path()
        ),
        format!("{}/authorize", h.campbooks.server.uri())
    );
    assert_eq!(param(&authorize, "client_id"), "applicamp-campbooks");
    assert_eq!(
        param(&authorize, "redirect_uri"),
        format!("{ORIGIN}/bff/callback/campbooks")
    );
    assert_eq!(
        param(&authorize, "scope"),
        "openid profile email offline_access"
    );
    assert_eq!(param(&authorize, "code_challenge_method"), "S256");
    let nonce = param(&authorize, "nonce");
    h.campbooks
        .answer(
            h.campbooks
                .sign(&h.campbooks.claims("cb-1", "Ana@example.com", &nonce)),
        )
        .await;

    let response = h
        .callback("campbooks", Some(&cookie), &param(&authorize, "state"))
        .await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/");
    assert!(clears(&response, "applicamp_login"));
    let session = cookie_named(&response, "applicamp_session").expect("a session cookie");
    let set_cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(
        !set_cookie.contains("provider-"),
        "no provider token reaches the browser"
    );

    // The code went back with the PKCE verifier of the challenge sent.
    let exchange = h.campbooks.token_requests().await;
    assert_eq!(exchange.len(), 1);
    let verifier = url::form_urlencoded::parse(exchange[0].as_bytes())
        .find(|(key, _)| key == "code_verifier")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    assert_eq!(
        applicamp_kernel::crypto::pkce_challenge(&verifier),
        param(&authorize, "code_challenge")
    );

    let (status, me) = h.me(Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "ana@example.com");
    assert_eq!(me["name"], "Ana Example");
    assert_eq!(me["ways_in"], json!(["campbooks"]));
    let me_text = me.to_string();
    assert!(!me_text.contains("provider-"), "no token in /api/v1/me");
}

#[sqlx::test(migrations = "../../migrations")]
async fn someone_not_on_the_list_lands_on_not_invited(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;

    let response = h.sign_in("linkedin", "li-1", "bo@example.com").await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), handlers::NOT_INVITED);
    assert!(cookie_named(&response, "applicamp_session").is_none());
    assert_eq!(h.count("SELECT count(*) FROM identity.accounts").await, 0);
    assert_eq!(h.count("SELECT count(*) FROM bff.sessions").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn both_ways_in_reach_one_account(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;

    let first = h.sign_in("campbooks", "cb-1", "ana@example.com").await;
    assert_eq!(location(&first), "/");
    let second = h.sign_in("linkedin", "li-1", "ana@example.com").await;
    assert_eq!(location(&second), "/");

    let session = cookie_named(&second, "applicamp_session").unwrap();
    let (status, me) = h.me(Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["ways_in"], json!(["campbooks", "linkedin"]));
    assert_eq!(h.count("SELECT count(*) FROM identity.accounts").await, 1);
}

/// Runs a callback whose ID token is `token(fake, nonce)` and checks that
/// it fails: no session, no account.
async fn assert_refused(h: &Harness, token: impl Fn(&Fake, &str) -> String) {
    let (cookie, authorize) = h.start("campbooks").await;
    let nonce = param(&authorize, "nonce");
    h.campbooks.answer(token(&h.campbooks, &nonce)).await;

    let response = h
        .callback("campbooks", Some(&cookie), &param(&authorize, "state"))
        .await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), handlers::SIGN_IN_FAILED);
    assert!(cookie_named(&response, "applicamp_session").is_none());
    assert!(clears(&response, "applicamp_login"));
    assert_eq!(h.count("SELECT count(*) FROM bff.sessions").await, 0);
    assert_eq!(h.count("SELECT count(*) FROM identity.accounts").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_bad_nonce_is_refused(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    assert_refused(&h, |fake, _nonce| {
        fake.sign(&fake.claims("cb-1", "ana@example.com", "another-nonce"))
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_wrong_audience_is_refused(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    assert_refused(&h, |fake, nonce| {
        let mut claims = fake.claims("cb-1", "ana@example.com", nonce);
        claims["aud"] = json!("someone-else");
        fake.sign(&claims)
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_wrong_issuer_is_refused(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    assert_refused(&h, |fake, nonce| {
        let mut claims = fake.claims("cb-1", "ana@example.com", nonce);
        claims["iss"] = json!("https://elsewhere.example.com");
        fake.sign(&claims)
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_forged_signature_is_refused(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    // Same key id, another key: the published key does not verify it.
    let forger = signing_key();
    assert_refused(&h, move |fake, nonce| {
        sign_with(&forger, &fake.claims("cb-1", "ana@example.com", nonce))
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_id_token_is_refused(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    assert_refused(&h, |fake, nonce| {
        let mut claims = fake.claims("cb-1", "ana@example.com", nonce);
        let past = jsonwebtoken::get_current_timestamp() - 3600;
        claims["iat"] = json!(past - 300);
        claims["exp"] = json!(past);
        fake.sign(&claims)
    })
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_forged_state_is_refused_before_any_exchange(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    let (cookie, authorize) = h.start("campbooks").await;
    let nonce = param(&authorize, "nonce");
    h.campbooks
        .answer(
            h.campbooks
                .sign(&h.campbooks.claims("cb-1", "ana@example.com", &nonce)),
        )
        .await;

    let forged = h.callback("campbooks", Some(&cookie), "forged-state").await;
    let no_cookie = h
        .callback("campbooks", None, &param(&authorize, "state"))
        .await;
    // A sign-in started with Campbooks cannot finish at LinkedIn's callback.
    let other_provider = h
        .callback("linkedin", Some(&cookie), &param(&authorize, "state"))
        .await;

    for response in [forged, no_cookie, other_provider] {
        assert_eq!(location(&response), handlers::SIGN_IN_FAILED);
        assert!(cookie_named(&response, "applicamp_session").is_none());
    }
    assert!(
        h.campbooks.token_requests().await.is_empty(),
        "no code was redeemed"
    );
    assert!(h.linkedin.token_requests().await.is_empty());
    assert_eq!(h.count("SELECT count(*) FROM bff.sessions").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_error_is_refused(db: PgPool) {
    let h = Harness::new(db).await;
    let (cookie, authorize) = h.start("linkedin").await;
    let response = h
        .get(
            &format!(
                "/bff/callback/linkedin?error=access_denied&state={}",
                param(&authorize, "state")
            ),
            Some(&cookie),
        )
        .await;
    assert_eq!(location(&response), handlers::SIGN_IN_FAILED);
    assert!(h.linkedin.token_requests().await.is_empty());
}

fn sign_out(session: Option<&str>, csrf: bool, origin: Option<&str>) -> Request<Body> {
    let mut request = Request::post("/bff/sign-out");
    if let Some(session) = session {
        request = request.header(header::COOKIE, session);
    }
    if csrf {
        request = request.header(CSRF_HEADER, "1");
    }
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    request.body(Body::empty()).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn signing_out_ends_the_session(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    let signed_in = h.sign_in("campbooks", "cb-1", "ana@example.com").await;
    let session = cookie_named(&signed_in, "applicamp_session").unwrap();

    let response = h.send(sign_out(Some(&session), true, Some(ORIGIN))).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(clears(&response, "applicamp_session"));
    assert_eq!(h.count("SELECT count(*) FROM bff.sessions").await, 0);
    let (status, problem) = h.me(Some(&session)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(problem["code"], "session_required");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_cross_site_request_is_refused_and_changes_nothing(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let h = Harness::new(db).await;
    let signed_in = h.sign_in("campbooks", "cb-1", "ana@example.com").await;
    let session = cookie_named(&signed_in, "applicamp_session").unwrap();

    for request in [
        sign_out(Some(&session), false, Some(ORIGIN)),
        sign_out(Some(&session), true, Some("https://evil.example.com")),
        sign_out(Some(&session), true, None),
    ] {
        let response = h.send(request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(body(response).await["code"], "csrf_check_failed");
    }

    assert_eq!(h.count("SELECT count(*) FROM bff.sessions").await, 1);
    assert_eq!(h.me(Some(&session)).await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_public_endpoints(db: PgPool) {
    let h = Harness::new(db).await;

    let providers = h.get("/bff/providers", None).await;
    assert_eq!(providers.status(), StatusCode::OK);
    assert_eq!(
        providers.headers()[header::CACHE_CONTROL].to_str().unwrap(),
        "no-store"
    );
    assert_eq!(
        body(providers).await,
        json!({ "providers": ["campbooks", "linkedin"] })
    );

    let health = h.get("/healthz", None).await;
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(body(health).await["status"], "ok");

    assert_eq!(
        h.get("/bff/sign-in/google", None).await.status(),
        StatusCode::NOT_FOUND
    );
    let (status, problem) = h.me(None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(problem["code"], "session_required");
    let missing = h.get("/nothing-here", None).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing.headers()[header::CONTENT_TYPE].to_str().unwrap(),
        "application/problem+json"
    );
}

#[test]
fn a_provider_needs_its_secret_and_campbooks_its_issuer() {
    let mut config = Config::for_database("postgres://unused");
    assert!(Settings::from_config(&config).unwrap().providers.is_empty());

    config.oidc.linkedin.client_id = Some("li".to_owned());
    let err = Settings::from_config(&config).unwrap_err().to_string();
    assert!(
        err.contains("APPLICAMP_OIDC__LINKEDIN__CLIENT_SECRET"),
        "{err}"
    );

    config.oidc.linkedin.client_secret = Some(SecretString::from("s".to_owned()));
    let settings = Settings::from_config(&config).unwrap();
    assert_eq!(settings.providers[0].issuer, super::LINKEDIN_ISSUER);
    assert!(!settings.secure);
    assert_eq!(settings.public_origin, "http://localhost:8080");

    config.oidc.campbooks.client_id = Some("cb".to_owned());
    config.oidc.campbooks.client_secret = Some(SecretString::from("s".to_owned()));
    let err = Settings::from_config(&config).unwrap_err().to_string();
    assert!(err.contains("APPLICAMP_OIDC__CAMPBOOKS__ISSUER"), "{err}");

    config.public_url = "https://applicamp.example.com/".to_owned();
    config.oidc.campbooks.issuer = Some("https://campbooks.example.com".to_owned());
    let settings = Settings::from_config(&config).unwrap();
    assert!(settings.secure);
    assert_eq!(settings.public_origin, "https://applicamp.example.com");
    assert_eq!(settings.providers.len(), 2);
}
