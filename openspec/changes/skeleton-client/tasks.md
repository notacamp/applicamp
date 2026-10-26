# Tasks

Needs `@notacamp/ui` published (notacamp/campbooks-cloud#371) and the
server of the change `skeleton-server`. The staging deploy is not part of
this change either: it is driven from notacamp/applicamp-cloud#1.

## 2. Client
- [ ] 2.1 Vite + React 19 + TS strict + TanStack Router/Query + generated API types (`pnpm api:check`) + i18next (4 languages) + ESLint rules from AGENTS.md + dependency-cruiser
- [ ] 2.2 Applicamp token set (light + dark), sidebar shell, ⌘K, ? sheet, empty states
- [ ] 2.3 Sign-in, not-invited and sign-out flows

## 3. Delivery
- [ ] 3.1 CI: client lint, typecheck, test and `api:check` join the gates
- [ ] 3.2 The Dockerfile's web stage builds `clients/app` and the server serves it (`APPLICAMP_WEB_DIR`)
- [ ] 3.3 E2E happy path against the compose stack with the fake OIDC provider (+ axe)

## 4. Finish
- [ ] 4.1 Client types regenerated, changelog entry file, gates green in CI, recording at 375px and desktop
