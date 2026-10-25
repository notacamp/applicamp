# 0001 — Spec-driven development with OpenSpec

**Status:** Accepted · 2026-10-25

## Context
Applicamp is built mostly by agents, reviewed by one person who checks the
finished result. Behaviour has to be written down where both can read it.

## Decision
Every behaviour change is an OpenSpec change carried in the same PR as its
code, tests, changelog entry file and recording, and archived before merge.
Same workflow as Campbooks Next. Docs-only, CI-only and version-bump PRs skip it.

## Consequences
`openspec/specs/` is the behaviour baseline. CI runs `openspec validate --all --strict`.
