# Design

## Data (schema `runs`)
- `runs(id uuid pk, account_id, kind text, status text, inputs jsonb, created_at, started_at, finished_at, sandbox text, image text, policy text, model text, tokens_in int default 0, tokens_out int default 0, error_kind text, summary jsonb)`.
- `events(id bigserial pk, run_id, at timestamptz, layer text check in ('agent','sandbox'), level text check in ('info','tool','model','deny','ask','error'), message text check (length(message) <= 2000), data jsonb)`; index `(run_id, id)`. Retention: 30 days (a daily job deletes older runs' events; runs keep their summary).
- `tokens(run_id pk, hash bytea, expires_at)`: the run token is 32 random bytes, sent once to the sandbox, stored as SHA-256; expires at the run's time limit + 5 minutes and is deleted on finish.
- Events published: `runs.finished` v1 `{ run, account, kind, status }`.

## Lifecycle
`POST /api/v1/runs { kind, inputs }` (session, CSRF) → queued job `runs.start`. The worker:
1. checks concurrency (else stays queued),
2. mints the run token,
3. `SandboxRuntime::create(template_for(kind), env { APPLICAMP_RUN_ID, APPLICAMP_RUN_TOKEN, APPLICAMP_AGENT_API })`,
4. streams the sandbox's log into `events` (layer `sandbox` for OpenShell's OCSF lines, `agent` for the runner's JSON lines on stdout),
5. waits for exit or the time limit,
6. **always** deletes the sandbox (also on error and on server restart: a startup sweep deletes sandboxes labelled `applicamp-run=*` whose run is finished or unknown),
7. records status, tokens and summary, publishes `runs.finished`.
`POST /api/v1/runs/{id}/cancel` stops it the same way.

## SandboxRuntime
```rust
trait SandboxRuntime: Send + Sync {
    async fn create(&self, spec: SandboxSpec) -> Result<SandboxHandle>;   // template name, image, env, labels
    fn logs(&self, handle: &SandboxHandle) -> BoxStream<'static, Result<LogLine>>;
    async fn wait(&self, handle: &SandboxHandle, limit: Duration) -> Result<ExitStatus>;
    async fn delete(&self, handle: &SandboxHandle) -> Result<()>;
    async fn sweep(&self, keep: &HashSet<Uuid>) -> Result<usize>;
}
```
- **OpenShell** (`openshell-sdk`, git tag `v0.1.4`, already allowed by cargo-deny): templates `applicamp-<kind>` created/updated at startup from `policies/<kind>.yaml` + per-kind limits (CPU 1, memory 1 GiB, pids 512, time limit per kind). Logs via `raw_grpc_fresh()` log stream. Config `APPLICAMP_SANDBOX__RUNTIME=openshell|fake`, `APPLICAMP_SANDBOX__GATEWAY_URL`, `APPLICAMP_SANDBOX__GATEWAY_CA_FILE` (TLS), auth = an OIDC bearer the server mints for itself (see below), `APPLICAMP_SANDBOX__AGENT_IMAGE`.
- **Gateway auth**: the gateway trusts Applicamp as its OIDC issuer. The server publishes `GET /.well-known/openid-configuration` and `GET /oauth2/jwks` (EdDSA key derived from the instance secret with its own purpose, `kid` stable) and mints 5-minute JWTs (`iss` = public URL, `aud` = `openshell-gateway`, roles claim `openshell-admin`) for its own gRPC calls. Nothing else is issued.
- **Fake**: runs `target/debug/applicamp-agent` (or the path in `APPLICAMP_SANDBOX__FAKE_AGENT`) as a child process with the same env; logs = its stdout. Used by tests and local runs; never in production (refuses to start when `APPLICAMP_PUBLIC_URL` is https unless `APPLICAMP_SANDBOX__ALLOW_FAKE=true`).

## Agent API (bearer run token; never the session cookie)
- `GET /api/agent/v1/context` → `{ run, kind, inputs, profile_version?, goals_version? }`.
- `POST /api/agent/v1/events` `[{ level, message, data? }]` (≤ 50 per call, message ≤ 2000 chars, data ≤ 8 KB).
- `POST /api/agent/v1/complete` `{ messages, response_format?, max_tokens ≤ 16000 }` → the `complete` role through `crates/ai` with the run's account cap; adds tokens to the run.
- `POST /api/agent/v1/finish` `{ status: succeeded|failed, summary }`.
- Every call checks the token, that the run is `running`, and the kind's tool allow-list. 404 for anything else. Rate limit 10 req/s per run.

## Agent runner (`crates/agent`)
One binary, `applicamp-agent`, reads the env, `GET context`, dispatches on `kind`, writes JSON lines to stdout (`{"level":"tool","message":"…"}`) and the same through `POST events`, calls `finish`. Kind `probe`: GET context → event; try `https://example.com` (must be refused by policy) → event `deny`; finish. Image: distroless, nonroot, `WORKDIR /sandbox`, built and published like the server (`publish.yml` builds both).

## Policies
`policies/probe.yaml` (OpenShell policy v1): allow `applicamp.not-a-camp.com:443` (from config: the public host) for `/app/applicamp-agent`, nothing else. A CI lint checks every policy is deny-by-default and never lists `linkedin.com`, `indeed.com` or private ranges.

## Client: Agents page
Prototype layout: runs on the left (kind, title, status dot, started), the console on the right with tabs **What it's doing** (agent events, live via `GET /api/v1/runs/{id}/events/stream` SSE, resumable with `Last-Event-ID`) and **Sandbox log** (sandbox layer), and run facts (sandbox, image, policy, model, tokens, duration). A "Run a check" button starts a `probe` run. Empty state when there are no runs. 375px + desktop, 4 languages.

## Telemetry
Server logs: run id, kind, status, durations; never event messages. Prometheus: `applicamp_runs_total{kind,status}`, `applicamp_run_duration_seconds{kind}`, `applicamp_sandbox_denies_total{kind}`, `applicamp_ai_tokens_total{role}`.
