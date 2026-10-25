# Design

## Workspace layout
```
Cargo.toml                 # workspace: edition 2024, resolver 3, workspace lints (forbid unsafe)
crates/kernel              # applicamp-kernel: config, error, db, ids, clock, telemetry, jobs, events, auth
crates/server              # applicamp-server: composes modules, BFF, OpenAPI, serves clients/app/dist
crates/modules/identity    # applicamp-identity
crates/xtask               # cargo xtask arch | openapi [--check]
migrations/                # <ts>_<module>__<desc>.sql
openapi/openapi.json       # generated, committed
clients/app                # Vite + React 19 client on @notacamp/ui
deploy/compose.yml         # server + postgres, for staging and the e2e stack
Dockerfile                 # web build + cargo-chef + distroless runtime
```
Copy from Campbooks Next (`origin/next`) and trim: `crates/kernel/src/{config,error,db,ids,clock,telemetry,events}.rs`, `jobs/`, `auth/` (session-scoped, no JWT issuer), `crates/server/src/bff/{cookies,csrf,oidc,store,handlers}.rs`, `crates/server/src/{telemetry,web,error_reports}.rs`, the `xtask` arch/openapi checks (only the rules AGENTS.md lists). Rename `campbooks` → `applicamp` everywhere, including env prefixes (`APPLICAMP_…`). Do not copy tenancy/workspaces, the tracking plan, admin, records or rate-limit modules.

## Data
Schema `identity`:
- `accounts(id uuid pk, email text not null, email_verified bool, name text, picture_url text, created_at, last_sign_in_at)`
- `ways_in(id, account_id → accounts, provider text check in ('campbooks','linkedin'), subject text, unique(provider, subject), created_at)` — one account can have both.
- `invites(email citext pk, invited_at, note text)` — the invite list; managed by CLI (`applicamp invite add|remove|list <email>`), no HTTP endpoint.
Schema `bff`: `sessions(id, account_id, created_at, expires_at, last_seen_at)` — httpOnly cookie, server-side session, 30-day sliding expiry.
Schema `kernel`: `jobs`, `events` (outbox) as in Campbooks.

## Sign-in
- Providers are config: `APPLICAMP_OIDC__CAMPBOOKS__{ISSUER,CLIENT_ID,CLIENT_SECRET_FILE}` and `APPLICAMP_OIDC__LINKEDIN__{CLIENT_ID,CLIENT_SECRET_FILE}` (LinkedIn issuer `https://www.linkedin.com/oauth`). A provider with no config is hidden.
- Authorization code + PKCE S256, `state` + `nonce` in a short-lived signed cookie (`SameSite=Lax`, needed for the cross-site redirect back); ID token validated against the provider's JWKS (iss, aud, exp, nonce).
- Scopes: Campbooks `openid profile email offline_access` now (mail and calendar scopes are added by the correspondence change, behind notacamp/campbooks-cloud#369); LinkedIn `openid profile email`.
- The account is found by (provider, subject); else by verified email **only if** the email is on the invite list; else a new account is created **only if** the verified email is on the invite list. Otherwise the user lands on "You're not on the list yet" and no account is created.
- Session cookie `applicamp_session`: httpOnly, Secure, `SameSite=Strict`, path `/`. Every unsafe request needs `X-Applicamp-CSRF: 1` and a same-origin `Origin`.
- Campbooks refresh tokens (for later API use) are stored encrypted with the instance secret (kernel `crypto`), never sent to the browser.

## Endpoints (all in OpenAPI)
- `GET /bff/sign-in/{provider}` → redirect to the provider. `GET /bff/callback/{provider}` → session + redirect to `/`.
- `POST /bff/sign-out` (CSRF). `GET /api/v1/me` (session) → `{ id, name, email, picture_url, ways_in: [provider] }`.
- `GET /healthz` (public).

## Client
Routes: `/sign-in` (two buttons, provider availability from `GET /bff/providers`), `/not-invited`, and the shell with `/` (Today), `/pipeline`, `/agents`, `/profile` showing designed empty states. Uses `@notacamp/ui` Shell, NavItem, EmptyState, Button, Avatar, CommandMenu (⌘K), ShortcutSheet (?). Theme: `clients/app/src/lib/theme/applicamp.css` overrides the package's tokens (Notion-like: white / #f7f7f5 sidebar / #37352f text, system sans; dark mode too). No component styling outside the package.

## Testing
- Rust: `#[sqlx::test]` per slice; a fake OIDC provider in tests (wiremock) for both providers.
- Client: Vitest for the shell and sign-in page.
- E2E (Playwright, one happy path): the compose stack in CI with a fake OIDC provider container replaced by a tiny in-repo fake (`clients/app/scripts/fake-oidc.ts`, Node) → sign in with Campbooks (fake) → land on Today → sign out. Plus an axe check on each page.

## Security
No tokens in the browser; no tokens in logs; ID tokens and refresh tokens never logged; invite list checked server-side; 404 for anything not yours.
