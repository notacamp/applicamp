# Design

**Chosen: a durable domain event, not a session-time check.** In one
transaction `invites::remove` deletes the invite and, if an account has that
address, publishes `identity.invite_revoked { account }` (ids only, no email).
The server registers a consumer, `bff.end_sessions`, that deletes the
account's rows in `bff.sessions`. Modules never touch the `bff` schema, the
outbox gives at-least-once delivery across restarts, and the delete is
idempotent. A session lookup that joined identity would need a cross-schema
read on every request.

- Data ownership: `identity` owns accounts, ways in and invites; `bff` (the
  server) owns sessions. Published: `identity.invite_revoked` v1. Consumed:
  `bff.end_sessions`.
- The event is also published when the address is not on the list but an
  account has it, so running the command again finishes an interrupted
  revocation.
- Sign-in is checked inside the sign-in transaction, so it is refused at once;
  only already-open sessions wait for the worker (seconds).
- No endpoint, no scope, no agent work, no AI. Logs carry account ids, never
  addresses.
