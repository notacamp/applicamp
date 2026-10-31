# Agent runs: every agent in its own sandbox, and you can watch it

Issues: https://github.com/notacamp/applicamp/issues/41 (harness, phase P1),
https://github.com/notacamp/applicamp/issues/5 (runs),
https://github.com/notacamp/applicamp/issues/6 (logs). Decisions: ADR 0002, 0003, 0009.

## Why
Every agent feature (research, applying, recruiter replies, the profile
wizard's research) needs the same machinery: start an agent in an isolated
sandbox with a narrow network policy, let it ask the server for what it needs
without holding any secret, record what it does, and always clean up. The
owner also wants to see what agents are doing.

## What changes
- A `runs` module: the run queue (`runs.runs`, claimed by an orchestrator
  loop), the lifecycle (queued → starting → running → one end state),
  concurrency (1 run at a time per account, 3 on the instance), a deadline per
  kind, cancel, and a sweep at startup.
- **Run kinds are declared by the module that owns their subject**, with
  kernel types: name, image, policy, limits (steps, wall clock, tokens), its
  tools (each tagged read, draft or outward, with a call cap and a JSON
  schema, handled by that module) and its output schema. `runs` only executes
  kinds. A module starts a run through a port and hears the end through the
  `runs.finished` event. This change ships one kind, `probe`, owned by `runs`.
- A `SandboxRuntime` trait with two implementations: **OpenShell** (Rust SDK,
  v0.1.6; the policy is set per sandbox when it is created) and a **fake** for
  tests and local development (the agent binary as a child process behind a
  local proxy that applies the same policy).
- The **agent API** (`/api/agent/v1/…`), the only way a sandbox reaches the
  server, authenticated by a short-lived run token that names the tools it may
  call: `GET context`, `POST events`, `POST tools/{name}`, `POST complete`
  (one completion through `applicamp-ai`, on the run's account and caps; tool
  calling comes in the next phase), `POST finish`.
- The gateway trusts Applicamp as its OIDC issuer: discovery, JWKS and
  five-minute tokens the server mints for itself. Nothing else is issued.
- The **agent runner** (`crates/agent`, image `ghcr.io/notacamp/applicamp-agent`):
  reads its context, does the kind's work, writes events, finishes. `probe`
  calls one tool, is refused a second call past its cap, and checks that a
  host outside its policy is blocked.
- **Policies as files** (`policies/<kind>.yaml`) and a **policy lint** in CI:
  deny by default, `enforcement: enforce` on every inspected endpoint,
  `landlock.compatibility: hard_requirement`, never LinkedIn, Indeed or a
  private range.
- **The Agents page**: the runs, and for the one you pick, what it's doing
  (live over SSE), the sandbox log (OpenShell's allow and deny lines), and its
  facts (sandbox, image, policy, model, tokens, cost, duration, end state).
  "Run a check" starts a `probe`.

## Modules touched
`runs` (new); `kernel` (run-kind types, the `kinds` list on `Module`); `ai`
(the agent API's `complete` uses it); `server` (composition, the `/api/agent`
door exempt from CSRF and cookies, `/.well-known` and `/oauth2` routes). New
crate `crates/agent`. Client: `modules/runs` (the Agents page).

## Non-goals
- Tool calling in `applicamp-ai`, the multi-step loop, context rules and evals
  (phase P2 of #41).
- Approvals and applying (#7), standing orders, the recruiter inbox (#8).
- Prometheus metrics and Grafana dashboards: there is no metrics registry yet;
  they come with their own change under #6.
