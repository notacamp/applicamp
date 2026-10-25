# Tasks

## 1. Server
- [ ] 1.1 Cargo workspace, lints, `kernel` (trimmed copy), `server` with `/healthz`, JSON logs, OpenAPI generation
- [ ] 1.2 Migrations for `kernel`, `bff`, `identity`; `#[sqlx::test]` harness
- [ ] 1.3 `xtask arch` (module deps, migration names, every operation has security) and `xtask openapi --check`
- [ ] 1.4 `identity`: accounts, ways in, invite list + CLI
- [ ] 1.5 BFF: OIDC sign-in for Campbooks and LinkedIn, callback, session, sign-out, CSRF; `/api/v1/me`; `/bff/providers`

## 2. Client (needs `@notacamp/ui` published, notacamp/campbooks-cloud#371)
- [ ] 2.1 Vite + React 19 + TS strict + TanStack Router/Query + generated API types (`pnpm api:check`) + i18next (4 languages) + ESLint rules from AGENTS.md + dependency-cruiser
- [ ] 2.2 Applicamp token set (light + dark), sidebar shell, ⌘K, ? sheet, empty states
- [ ] 2.3 Sign-in, not-invited and sign-out flows

## 3. Delivery
- [ ] 3.1 CI: fmt, clippy -D warnings, xtask, nextest, cargo-deny, gitleaks, client lint/typecheck/test/api:check, openspec validate, PR title, changelog check
- [ ] 3.2 Dockerfile (distroless, arm64 + amd64), publish to `ghcr.io/notacamp/applicamp` on main, Trivy
- [ ] 3.3 E2E happy path against the compose stack with the fake OIDC provider (+ axe)
- [ ] 3.4 Staging deploy (private ops repo), demo data only

## 4. Finish
- [ ] 4.1 OpenAPI regenerated, client types regenerated, changelog entry file, gates green in CI, recording at 375px and desktop
