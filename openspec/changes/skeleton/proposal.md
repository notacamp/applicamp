# Skeleton: a running, signed-in, deployable Applicamp

Issue: https://github.com/notacamp/applicamp/issues/3

## Why
Every later step (profile, search agents, CVs, applying, email) needs the same
base: a server with modules, a database, a job queue, sign-in, a client shell,
CI gates, an image and a staging deploy. Building it once, with the rules of
AGENTS.md enforced from the first commit, keeps every later PR small.

## What changes
- A Rust workspace: `kernel`, `server`, `xtask`, and the first module,
  `identity`. Copied and trimmed from Campbooks Next (config, errors, DB,
  ids, clock, telemetry, Postgres job queue and outbox, BFF session + CSRF,
  OIDC client).
- Sign-in with **Campbooks** and with **LinkedIn** (both OIDC), limited to an
  invite list; sign-out; the "you're not on the list yet" page.
- The client shell on `@notacamp/ui`: sidebar (Today, Pipeline, Agents,
  Profile & goals), empty states, the Applicamp token set (Notion-like
  neutrals), en / pt-PT / es-ES / fr.
- Gates in CI; one image; a staging deploy with demo data.

## Modules touched
`identity` (new). Kernel and server are new.

## Non-goals
- Profile import, goals, search, CVs, runs, approvals, mail (their own changes).
- Native apps (Tauri). Applicamp is web-only for now.
- Open sign-up, workspaces or teams: one account per person, invite list only.
- A production deploy (issue #10).
