# Design

## Run kinds (kernel)
`applicamp_kernel::agent` holds the types a module uses to declare a kind, so
no module depends on `runs`:

```rust
pub struct RunKind {
    pub name: &'static str,          // "<module>.<kind>", e.g. "runs.probe"
    pub image: Image,                // Agent | Browser
    pub policy: &'static str,        // policies/<kind>.yaml, embedded
    pub limits: Limits,              // max_steps, wall_clock, max_tokens
    pub tools: Vec<ToolSpec>,
    pub output: serde_json::Value,   // JSON schema of finish's summary
    pub manual: bool,                // a person may start it (Agents page)
}
pub struct ToolSpec { pub name: &'static str, pub class: ToolClass, pub cap: u32,
                      pub schema: serde_json::Value, pub handler: Arc<dyn ToolHandler> }
pub enum ToolClass { Read, Draft, Outward }
pub trait ToolHandler: Send + Sync {
    fn call<'a>(&'a self, ctx: ToolContext<'a>, args: Value) -> BoxFuture<'a, Result<Value, ToolError>>;
}
```

`Module` gains `kinds: Vec<RunKind>`; the server gathers every module's kinds
and hands them to `runs`. An outward tool may only create an approval (once
the approvals module exists); `runs` refuses to register one otherwise. A
module starts a run through a port the server implements
(`start_run(conn, StartRun { kind, account, subject, inputs }) -> RunId`, in
the caller's transaction) and hears the end by consuming `runs.finished`.

## Data (schema `runs`)
- `runs(id uuid pk, account_id, kind, subject_id uuid null, status text, outcome text null, inputs jsonb, deadline timestamptz, created_at, started_at, finished_at, sandbox text, image text, policy text, model text, tokens_in int default 0, tokens_out int default 0, cost_eur double precision default 0 (as `ai.usage.cost_usd`), error_kind text, summary jsonb, cancel_requested bool default false, claimed_by text, claimed_until timestamptz, last_seq bigint default 0)`.
  `status`: `queued | starting | running | finished`. `outcome` (when finished): `succeeded | nothing_new | needs_you | ended_max_steps | ended_budget | ended_timeout | ended_loop | failed | cancelled`.
- `events(run_id, seq bigint, agent_seq bigint null unique per run, at, layer text check in ('agent','sandbox'), level text check in ('info','tool','model','deny','ask','error'), operation text null, tool_name text null, message text check (length(message) <= 2000), data jsonb, primary key (run_id, seq))`. `seq` is the SSE `Last-Event-ID`, given out from `runs.last_seq`; agent events carry their own number (`agent_seq`) so a line seen on stdout and through `POST events` is stored once. Retention: 30 days (a daily job deletes older runs' events; runs keep their summary).
- `tokens(run_id pk, hash bytea, tools jsonb, calls jsonb, expires_at)`: 32 random bytes sent once to the sandbox, stored as SHA-256, with the kind's tools and their caps and a count of calls made; expires at the deadline + 5 minutes; deleted on finish.
- Events published: `runs.finished` v1 `{ run, account, kind, subject, outcome }`.

## Lifecycle
A run outlives the kernel job lease (60 s), so the queue is `runs.runs` itself:
the server starts an **orchestrator loop** that claims queued runs with
`FOR UPDATE SKIP LOCKED` (and a renewed claim), within concurrency (1 per
account, 3 per instance). For each claimed run it:
1. mints the run token,
2. `SandboxRuntime::create(spec)`: image and template for the kind, the
   kind's policy, the run id, token and agent API address as command
   arguments (a gateway template cannot carry per-run environment), labels
   `applicamp-run=<id>`,
3. follows the sandbox's log into `events` (layer `sandbox` for OpenShell's
   OCSF lines, `agent` for the runner's JSON lines),
4. waits for exit, the deadline, or a cancel,
5. **always** deletes the sandbox, also on error,
6. records the outcome, tokens, cost and summary, deletes the token, and
   publishes `runs.finished`.
A claim lasts 30 s and is renewed every second while its run goes (the same
query reads Stop). A run whose claim lapsed (its server died) is marked
`failed` (`error_kind = server_restarted`) at startup and by every turn of the
loop; then a sweep deletes the sandboxes labelled `applicamp=run` whose
`applicamp-run=<id>` is not a run going (at startup, after each such end, and
every five minutes). OpenShell's label selector has no "exists" operator,
hence the two labels.
`POST /api/v1/runs { kind }` (session, CSRF) queues a run a person may start
(`runs.probe` only, for now); `POST /api/v1/runs/{id}/cancel` asks it to stop.

## SandboxRuntime
```rust
#[async_trait]
trait SandboxRuntime: Send + Sync {
    async fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle>;
    fn logs(&self, handle: &SandboxHandle) -> BoxStream<'static, Result<LogLine>>;
    async fn wait(&self, handle: &SandboxHandle, until: Instant) -> Result<Exit>;
    async fn delete(&self, handle: &SandboxHandle) -> Result<()>;
    async fn sweep(&self, keep: &HashSet<Uuid>) -> Result<usize>;
}
```
- **OpenShell** (`openshell-sdk`, `openshell-policy`, git tag `v0.1.6`): one
  template `applicamp-<kind>` per kind for the image and resources (CPU 1,
  memory 1 GiB), created or replaced at startup (there is no update RPC: get,
  compare, delete and create). The policy is parsed from the kind's file with
  `openshell_policy::parse_sandbox_policy` and set on each sandbox at create,
  so a later kind can narrow it to its subject's host. Logs through
  `OpenShellClient::watch_logs` (resumable); OCSF lines arrive with
  `source: "sandbox"`, `level: "OCSF"`. Config `APPLICAMP_SANDBOX__RUNTIME=openshell|fake`,
  `APPLICAMP_SANDBOX__GATEWAY_URL`, `APPLICAMP_SANDBOX__GATEWAY_CA_FILE`,
  `APPLICAMP_SANDBOX__AGENT_IMAGE`.
- **Gateway auth**: the gateway trusts Applicamp as its OIDC issuer. The
  server serves `GET /.well-known/openid-configuration` and `GET /oauth2/jwks`
  (an Ed25519 key derived from the instance secret for this purpose only,
  stable `kid`) and mints five-minute JWTs (`iss` = the public origin, `aud` =
  `openshell-gateway`, `sub` = `applicamp-server`, `roles` =
  `["openshell-admin"]`) for its own gRPC calls. Nothing else is issued.
- **Unset** `APPLICAMP_SANDBOX__RUNTIME`: no runtime; runs cannot start
  (`503 sandboxes_unavailable`) and the Agents page says so (`available`).
- **Fake**: runs the agent binary (`APPLICAMP_SANDBOX__FAKE_AGENT`, default
  `target/debug/applicamp-agent`) as a child process with the same
  arguments, behind a local CONNECT proxy that applies the kind's policy
  hosts and writes OCSF-style `DENIED` lines into the sandbox log. Logs are its
  stdout; the agent API is reached directly (`NO_PROXY`), with an empty
environment otherwise. The server image carries the runner at
`/app/applicamp-agent` for it (e2e, local development). Never in production: it refuses to start when `APPLICAMP_PUBLIC_URL`
  is https unless `APPLICAMP_SANDBOX__ALLOW_FAKE=true`.

## Agent API (bearer run token; a request with a cookie is refused)
- `GET /api/agent/v1/context` → `{ run, kind, inputs, subject, limits, tools: [{ name, class, schema, cap }] }`.
- `POST /api/agent/v1/events` `[{ seq, level, message, operation?, tool_name?, data? }]` (≤ 50 per call, message ≤ 2000 chars, data ≤ 8 KB).
- `POST /api/agent/v1/tools/{name}` `{ args }` → the handler's result. Refused (404) when the tool is not the kind's; `tool_cap_reached` past its cap; `invalid_arguments` (with the schema error) when `args` does not match.
- `POST /api/agent/v1/complete` `{ messages, response_format?, max_tokens ≤ 16000 }` → the `complete` role through `applicamp-ai` with the run's account and caps (refusals keep the gateway's codes: `ai_daily_limit_reached`, `ai_budget_reached`, recorded as an event); the run's tokens and cost grow; when its tokens plus this call's `max_tokens` would pass the kind's `max_tokens` the answer is `429 run_budget_reached`.
- `POST /api/agent/v1/finish` `{ outcome: succeeded | nothing_new | needs_you | failed, summary }` (summary checked against the kind's output schema).
- Every call checks the token's hash and expiry and that the run is `running` and not yet finished by its agent; 404 otherwise (401 `run_token_required` without a bearer, 400 `cookies_refused` with a cookie). 10 requests a second per run (`429 rate_limited`). `tool_cap_reached` is 409; bad arguments do not count against the cap.
- The door is exempt from the CSRF middleware and refuses `Cookie`; `cargo xtask arch` knows a `run_token` security scheme allowed only under `/api/agent/`.

## Agent runner (`crates/agent`)
One binary, `applicamp-agent --run <id> --token-file <path> --api <url>`
(or the token on stdin), depending on no other Applicamp crate. It reads its
context, dispatches on `kind`, writes JSON lines to stdout
(`{"seq":1,"level":"tool","message":"…"}`) and the same through `POST events`,
and calls `finish`. Kind `runs.probe`: `GET context` → event; call `probe_echo`
→ event; call it again and expect `tool_cap_reached` → event; fetch
`https://example.com/` and expect it blocked → event `deny`; finish
`succeeded`. Image: distroless, nonroot, `WORKDIR /sandbox`, published next to
the server image by `publish.yml`.

## Policies and the lint
`policies/probe.yaml` (OpenShell policy v1): the public Applicamp host on 443
for `/app/applicamp-agent`, with `enforcement: enforce`, and nothing else;
`landlock.compatibility: hard_requirement`. `cargo xtask policies` parses each
file with `openshell-policy` and fails on: an endpoint without `enforce`, a
missing `hard_requirement`, a wildcard host, a host under `linkedin.com` or
`indeed.com`, or a private, loopback or link-local range. It runs in CI next to
`cargo xtask arch`.

## Client: Agents page
The runs on the left (kind, status dot, started); the run on the right with
tabs **What it's doing** (agent events, live over
`GET /api/v1/runs/{id}/events/stream`, resumable with `Last-Event-ID`) and
**Sandbox log** (sandbox events), and the run facts (sandbox, image, policy,
model, tokens, cost, duration, end state). On phones the list and the run are
one column with a back button (`SplitView`). "Run a check" starts a probe;
"Stop" cancels a live run. An empty state when there are no runs. 375px and
desktop, four languages.

## Telemetry
Server logs: run id, kind, status, outcome, durations, counts; never event
messages, tool arguments or results.
