# Revoke on uninvite: removing an invite cuts off sign-in and ends sessions

Issue: https://github.com/notacamp/applicamp/issues/13

## Why
Today `applicamp invite remove` only stops new accounts. A provider identity
that is already linked still signs in, and sessions already open stay valid
for up to 30 days. Removing an invite must revoke access (security first).

## What changes
- A linked way in signs in only while its account's address is on the invite
  list; otherwise the result is "not invited" and nothing changes.
- `applicamp invite remove <email>` ends every session of the account that has
  that address, as soon as the server's worker handles the event.
- Re-inviting restores access to the same account. Nothing is deleted.

## Modules touched
`identity` (sign-in rule, `invites::remove`, new event `identity.invite_revoked`)
and the server's BFF (a consumer, `bff.end_sessions`, deletes the sessions).

## Non-goals
- Deleting accounts, ways in or any data.
- An admin UI or HTTP endpoint for the invite list (still command line only).
- Ending sessions synchronously inside the CLI: the BFF owns `bff.sessions`
  and the CLI only records the event; the running server applies it.
