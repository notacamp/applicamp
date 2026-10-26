# Tasks

The client tasks (2.x) and the e2e suite (3.3) are in the change
`skeleton-client`. The staging deploy (3.4) moved out of this repo: it is
driven from the private cloud repo, notacamp/applicamp-cloud#1.

## 1. Server
- [x] 1.1 Cargo workspace, lints, `kernel` (trimmed copy), `server` with `/healthz`, JSON logs, OpenAPI generation
- [x] 1.2 Migrations for `kernel`, `bff`, `identity`; `#[sqlx::test]` harness
- [x] 1.3 `xtask arch` (module deps, migration names, every operation has security) and `xtask openapi --check`
- [x] 1.4 `identity`: accounts, ways in, invite list + CLI
- [x] 1.5 BFF: OIDC sign-in for Campbooks and LinkedIn, callback, session, sign-out, CSRF; `/api/v1/me`; `/bff/providers`

## 3. Delivery
- [x] 3.1 CI: fmt, clippy -D warnings, xtask, nextest, cargo-deny, gitleaks, openspec validate, PR title, changelog check (the client gates join with `skeleton-client`)
- [x] 3.2 Dockerfile (distroless, arm64 + amd64), publish to `ghcr.io/notacamp/applicamp` on main, Trivy

## 4. Finish
- [x] 4.1 OpenAPI regenerated, changelog entry file, gates green in CI, terminal recording
