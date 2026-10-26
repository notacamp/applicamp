# Skeleton, server half: a running, signed-in, deployable Applicamp server

Issue: https://github.com/notacamp/applicamp/issues/3 (PR 1 of 2; the client
half is the change `skeleton-client`)

## Why
Every later step (profile, search agents, CVs, applying, email) needs the same
base: a server with modules, a database, a job queue, sign-in, CI gates and an
image. Building it once, with the rules of AGENTS.md enforced from the first
commit, keeps every later PR small. The client shell waits for `@notacamp/ui`
to be published (notacamp/campbooks-cloud#371), so the skeleton ships in two
changes: this one (the server) and `skeleton-client`.

## What changes
- A Rust workspace: `kernel`, `server`, `xtask`, and the first module,
  `identity`. Copied and trimmed from Campbooks Next (config, errors, DB,
  ids, clock, JSON logs, Postgres job queue and outbox, instance secret,
  BFF session + CSRF, OIDC client).
- Sign-in with **Campbooks** and with **LinkedIn** (both OIDC), limited to an
  invite list managed from the command line; sign-out; `/api/v1/me`.
- The OpenAPI document generated from the routes and committed.
- Gates in CI (fmt, clippy, architecture, OpenAPI drift, tests, cargo-deny,
  gitleaks, OpenSpec, PR title, changelog entry) and one image published on
  every push to `main`.

## Modules touched
`identity` (new). Kernel and server are new.

## Non-goals
- The client shell, its sign-in and not-invited pages and the e2e suite
  (change `skeleton-client`).
- The staging deploy: it is driven from the private cloud repo
  (notacamp/applicamp-cloud#1).
- Profile import, goals, search, CVs, runs, approvals, mail (their own changes).
- Open sign-up, workspaces or teams: one account per person, invite list only.
- A production deploy (issue #10).
