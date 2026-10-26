# Design

## Workspace layout
```
Cargo.toml                 # workspace: edition 2024, resolver 3, workspace lints (forbid unsafe)
crates/kernel              # applicamp-kernel: config, error, db, ids, clock, telemetry, jobs, events, crypto, auth
crates/server              # applicamp-server: composes modules, BFF, OpenAPI; the `applicamp` binary
crates/modules/identity    # applicamp-identity
crates/xtask               # cargo xtask arch | openapi [--check]
migrations/                # <ts>_<owner>__<desc>.sql
openapi/openapi.json       # generated, committed
deploy/compose.yml         # server + postgres, for local use and the e2e stack
Dockerfile                 # cargo-chef + distroless runtime (the web stage comes with skeleton-client)
```
Copied from Campbooks Next (`origin/next`) and trimmed: the kernel keeps
config, errors, DB, ids, clock, JSON logs, the job queue, the outbox and the
instance secret; the server keeps the BFF's cookies, CSRF check, OIDC client
and session store. Renamed `campbooks` → `applicamp` everywhere, including
env prefixes (`APPLICAMP_…`). Not copied: tenancy/workspaces and row-level
security, the tracking plan, admin, records, rate limits, the token issuer,
server-side message catalogues (problems are English; clients translate by
`code`), OpenTelemetry export.

## Data
Every id is a UUIDv7 made by the server. No table references another
schema.

Schema `identity` (module `identity`):
- `accounts(id uuid pk, email text not null unique, email_verified bool, name text, picture_url text, created_at, last_sign_in_at)`.
  `email` is the verified address the account was created with, lower-cased.
- `ways_in(id uuid pk, account_id → accounts, provider text check in ('campbooks','linkedin'), subject text, unique(provider, subject), refresh_token bytea, created_at, last_used_at)`. One account can have both.
- `invites(email text pk check (email = lower(email)), invited_at, note text)`: the invite list. Managed by the CLI (`applicamp invite add|remove|list`), no HTTP endpoint. Lower-cased text rather than `citext`, so no extension is needed.

Schema `bff` (the server's backend-for-frontend):
- `sessions(id uuid pk, token_hash bytea unique, account_id uuid, created_at, last_seen_at, expires_at)`. The cookie holds a random 256-bit token; only its SHA-256 is stored. 30-day sliding expiry: each use (at most once an hour) moves `expires_at` 30 days on.

Schema `kernel`:
- `events(id, name, version, payload jsonb, occurred_at)`: the outbox, written in the publisher's transaction together with one job per consumer.
- `jobs(id, kind, queue, payload jsonb, event_id, state, run_at, attempts, max_attempts, locked_until, last_error, created_at, finished_at)`: the job queue. Workers claim with `FOR UPDATE SKIP LOCKED`, lease, retry with backoff, and mark failed after `max_attempts`.

Events: `identity.account_created` (`{ account }`, ids only), published when
sign-in creates an account. No consumer yet.

## Sign-in
- Providers are config: `APPLICAMP_OIDC__CAMPBOOKS__{ISSUER,CLIENT_ID,CLIENT_SECRET_FILE}` and `APPLICAMP_OIDC__LINKEDIN__{CLIENT_ID,CLIENT_SECRET_FILE}` (LinkedIn issuer `https://www.linkedin.com/oauth`, overridable with `APPLICAMP_OIDC__LINKEDIN__ISSUER` for tests). `…__CLIENT_SECRET` works too, for development. A provider with no client id is hidden.
- Endpoints come from the provider's discovery document (`<issuer>/.well-known/openid-configuration`), whose `issuer` must equal the configured one.
- Authorization code + PKCE S256. `state`, `nonce`, the PKCE verifier and the provider are sealed (AES-256-GCM under the instance secret) into a 10-minute login cookie (`SameSite=Lax`, needed for the cross-site redirect back), which the callback clears whatever happens.
- The callback checks `state` against the cookie (constant time), redeems the code (`client_secret_post`), and validates the ID token against the provider's JWKS: signature (EdDSA or RS256, by `kid`), `iss`, `aud` (and `azp` when present), `exp`, and the `nonce`.
- Scopes: Campbooks `openid profile email offline_access` now (mail and calendar scopes are added by the correspondence change, behind notacamp/campbooks-cloud#369); LinkedIn `openid profile email`.
- The account is found by (provider, subject); else by verified email **only if** the email is on the invite list (the provider is linked to it); else a new account is created **only if** the verified email is on the invite list. Otherwise the browser is sent to `/not-invited` and no account is created.
- A failed callback (forged or expired state, refused code, invalid ID token, provider error) sends the browser to `/sign-in?error=sign_in_failed`. Logs name the provider and the failure kind, never a token, code or address.
- Session cookie `applicamp_session` (`__Host-applicamp_session` over https): httpOnly, `SameSite=Strict`, path `/`, `Secure` over https. Every unsafe request needs `X-Applicamp-CSRF: 1` and an `Origin` equal to `APPLICAMP_PUBLIC_URL`'s origin (and no cross-site `Sec-Fetch-Site`); otherwise 403 `csrf_check_failed`.
- Campbooks refresh tokens (for later API use) are stored on the way in, sealed with the instance secret (kernel `crypto`) and bound to the row; never sent to the browser.
- The instance secret: 32 random bytes, base64, in `APPLICAMP_SECRET_KEY_FILE` (or `APPLICAMP_SECRET_KEY`). The server refuses to start without it.

## Endpoints (all in OpenAPI)
- `GET /bff/providers` (public) → `{ providers: ["campbooks", "linkedin"] }`, the configured ones.
- `GET /bff/sign-in/{provider}` (public) → 303 to the provider. `GET /bff/callback/{provider}` (public) → session + 303 to `/` (or `/not-invited`, or `/sign-in?error=…`).
- `POST /bff/sign-out` (session, CSRF) → 204; the session row is deleted and the cookie cleared.
- `GET /api/v1/me` (session) → `{ id, name, email, picture_url, ways_in: [provider] }`; 401 `session_required` without a live session.
- `GET /healthz` (public) → `{ status, version }`, 503 when the database does not answer.

Every operation declares its security: `security(())` (public) or the
`session` cookie scheme. `cargo xtask arch` fails on an operation that
declares neither, and on a route added outside `utoipa-axum`'s `routes!`
(it would be missing from the document).

## CLI
One binary, `applicamp`: `serve` (HTTP API and the job worker; the
default), `invite add <email> [--note]`, `invite remove <email>`,
`invite list`, `openapi` (prints the document), `healthcheck` (probes
`/healthz` on 127.0.0.1; the runtime image has no shell).

## Testing
- Rust: `#[sqlx::test]` per slice; a fake OIDC provider (wiremock: discovery, JWKS, token endpoint) for both providers. Covers first sign-in, not invited, both ways in on one account, forged state, bad nonce, wrong audience, sign-out, the cross-site request and the invite CLI's functions.

## Security
No tokens in the browser; no tokens in logs; ID tokens and refresh tokens
never logged; invite list checked server-side; 404 for anything not yours.
