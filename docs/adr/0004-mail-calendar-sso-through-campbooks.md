# 0004 — Mail, calendar and SSO come from Campbooks

**Status:** Accepted · 2026-10-25

## Context
Applicamp needs to read recruiter threads, draft and send replies, and find
interview slots. Holding Gmail tokens ourselves would mean a second Google
restricted-scope security review and a second mail-sync engine. Campbooks
Next already syncs Gmail, Zoho and IMAP, exposes a scoped API (conversations,
drafts with send and undo, labels, a changes feed, schedule) and is an
OAuth 2.1 / OIDC issuer.

## Decision
- **Sign in with Campbooks** (OIDC, code + PKCE, confidential client) is the
  primary sign-in, and its consent grants `conversations:read`,
  `conversations:write`, `conversations:send`, `schedule:read`,
  `schedule:write`. **Sign in with LinkedIn** (OIDC: name, email, photo) is
  a second way in.
- Recruiter threads carry a "Job hunt" label in Campbooks; Applicamp lists them
  by label and follows the `changes` feed.
- Replies are Campbooks drafts; the user approves in Applicamp, which calls
  send. Campbooks' undo window still applies.
- Applicamp never holds a mailbox token.

## Needs in Campbooks
notacamp/campbooks-cloud#369 (instance-registered OAuth apps) and #370 (label
filter; later a permission limited to one label).

## Consequences
Applicamp depends on Campbooks being up. Until #370's label-limited
permission exists, the token can read the whole inbox; acceptable while the
only user is the owner, required before anyone else.
