# 0008 — Rust modular monolith + slim React client

**Status:** Accepted · 2026-10-25

## Decision
- Server: Rust 2024, axum, tokio, sqlx on Postgres, utoipa OpenAPI, figment,
  problem+json, Postgres job queue and outbox: the shape of Campbooks Next,
  with its kernel and AI gateway copied and trimmed.
- Client: the patterns of connect-web-v2, slimmed: Vite, React 19, strict
  TypeScript, TanStack Router + Query, generated API types with a drift check,
  zod, Vitest next to the code, one Playwright happy-path e2e suite against
  the real stack. Dropped: Storybook, visual regression, sharded and edge-case
  e2e, Gitflow release trains.
- One image, Docker Compose, the same infrastructure as Campbooks.
