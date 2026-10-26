# Agent runs: every agent in its own sandbox, and you can watch it

Issues: https://github.com/notacamp/applicamp/issues/5 (runs), https://github.com/notacamp/applicamp/issues/6 (logs)

## Why
Every agent feature (search, CVs, applying) needs the same machinery: start an
agent in an isolated sandbox with a narrow network policy, let it ask the
server for what it needs without holding any secret, record what it does, and
always clean up. The owner also wants to see what agents are doing in the
background.

## What changes
- A `runs` module: run kinds, a run queue, the lifecycle (queued → starting →
  running → succeeded / failed / cancelled / timed_out), per-account
  concurrency (1 run at a time per account, 3 on the instance) and a time
  limit per kind.
- A `SandboxRuntime` trait with two implementations: **OpenShell** (gRPC
  SDK, one sandbox template per run kind carrying its policy and limits) and
  a **fake** for tests and local development (runs the agent binary as a
  child process).
- The **agent API** (`/api/agent/v1/…`): the only door a sandbox has into the
  server, authenticated by a short-lived single-run bearer token. First tools:
  `GET context` (the run's inputs and the profile/goals versions it may read),
  `POST events` (structured run events), `POST complete` (model calls
  proxied by the server: the key never enters a sandbox; the per-account
  daily cap and usage ledger apply), `POST finish`.
- The **agent runner** (`crates/agent`, image `ghcr.io/notacamp/applicamp-agent`):
  a small Rust binary that runs one kind of work, writes events, and exits.
  First kind: `probe` (checks it can reach the agent API and is refused
  elsewhere; used by the health page and by tests).
- **Logs**: run events stored in `runs.events` and streamed live over SSE to
  the **Agents** page (run list + "What it's doing" + "Sandbox log", the
  OpenShell allow/deny lines, + run facts: sandbox, image, policy, model,
  tokens, duration). Server logs carry run ids and kinds only.
- Policies as files: `policies/<kind>.yaml` (deny by default; `probe` may
  reach only the public Applicamp host).

## Modules touched
`runs` (new), `ai` (the proxy endpoint uses the gateway). New crate
`crates/agent`. Client: `modules/runs` (the Agents page).

## Non-goals
- Job search, CVs and applying (their own changes; they add run kinds).
- Schedules (the nightly search comes with search).
- Grafana dashboards (ops repo; this change only makes the logs and metrics
  available).
