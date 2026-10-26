# Design

## Layout
```
clients/app                # Vite + React 19 client on @notacamp/ui
clients/app/scripts/fake-oidc.ts   # the e2e stack's fake OIDC provider (Node)
```
The Dockerfile's commented web stage (left by `skeleton-server`) builds
`clients/app`; the server serves `clients/app/dist` from `APPLICAMP_WEB_DIR`
with an `index.html` fallback for client routes.

## Client
Routes: `/sign-in` (one button per provider in `GET /bff/providers`, linking
to `/bff/sign-in/{provider}`; `?error=sign_in_failed` shows why it failed),
`/not-invited`, and the shell with `/` (Today), `/pipeline`, `/agents`,
`/profile` showing designed empty states. Who is signed in comes from
`GET /api/v1/me` (401 → `/sign-in`); sign-out is `POST /bff/sign-out` with
`X-Applicamp-CSRF: 1`, then `/sign-in`. Uses `@notacamp/ui` Shell, NavItem,
EmptyState, Button, Avatar, CommandMenu (⌘K), ShortcutSheet (?). Theme:
`clients/app/src/lib/theme/applicamp.css` overrides the package's tokens
(Notion-like: white / #f7f7f5 sidebar / #37352f text, system sans; dark
mode too). No component styling outside the package. API types are
generated from `openapi/openapi.json` (`pnpm api:check`).

## Testing
- Client: Vitest for the shell and sign-in page.
- E2E (Playwright, one happy path): the compose stack (`deploy/compose.yml`)
  in CI with a tiny in-repo fake OIDC provider (`clients/app/scripts/fake-oidc.ts`,
  Node) as Campbooks → sign in with Campbooks (fake) → land on Today → sign
  out. Plus an axe check on each page.
