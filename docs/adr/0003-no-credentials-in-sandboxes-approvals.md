# 0003 — Sandboxes hold no credentials; outward actions need an approval

**Status:** Accepted · 2026-10-25

## Context
Job pages and recruiter emails are untrusted text that can carry prompt
injection. A wrong application or email goes out under the user's name.

## Decision
- No token, key or password enters a sandbox. Agents get a short-lived run
  token that can call only that run's tools on the server's agent API.
- Submitting an application and sending an email are **approvals**: the agent
  requests one, the user accepts or declines on Today, the server executes.
  For applications the sandbox waits paused before Submit and resumes on
  accept; on timeout the user gets a prefilled link.
- No CAPTCHA solving or anti-bot evasion: a form that needs a human goes to the human.

## Consequences
Slower than fully automatic applying, by design. A later per-company
"auto-submit when the form has only standard fields" switch would need its own ADR.
