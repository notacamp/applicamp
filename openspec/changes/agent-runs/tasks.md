# Tasks
## 1. Kernel and server
- [x] 1.1 `kernel::agent`: `RunKind`, `ToolSpec`, `ToolClass`, `ToolHandler`, `Limits`; `Module.kinds`; the `start_run` port shape
- [x] 1.2 Server: gather kinds, compose `runs`, `/api/agent` door (no CSRF, no cookies), `/.well-known` and `/oauth2` prefixes; `cargo xtask arch` knows the `run_token` scheme
## 2. Runs module
- [x] 2.1 Schema, orchestrator loop (claims, concurrency, deadlines), cancel, startup sweep, `runs.finished`
- [x] 2.2 `SandboxRuntime` + the fake runtime (child process, policy proxy, OCSF-style deny lines)
- [x] 2.3 OpenShell runtime at v0.1.6 (templates per kind, policy per sandbox, `watch_logs`, delete, sweep) + the OIDC issuer for the gateway
- [x] 2.4 Agent API with run tokens (context, events, tools with caps and schemas, complete through `applicamp-ai`, finish), rate limit
- [x] 2.5 Run routes: start, list, get, cancel, events, live events (SSE); event retention job
## 3. Agent
- [x] 3.1 `crates/agent` runner + the `runs.probe` kind and its `probe_echo` tool; agent image in `publish.yml`
- [x] 3.2 `policies/probe.yaml` + `cargo xtask policies` in CI
## 4. Client
- [ ] 4.1 Agents page: run list, live console (SSE), sandbox log, run facts, Run a check, Stop
## 5. Finish
- [ ] 5.1 Tests: lifecycle with the fake runtime (success, failure, timeout, cancel, sweep), token checks (wrong run, finished run, expired, tool not allowed, cap reached, bad arguments), cap enforcement on `complete`; e2e: Run a check → succeeded with its events and a blocked request visible (desktop + phone)
- [ ] 5.2 OpenAPI + client types regenerated, changelog entry, gates green in CI, recording at 375px and desktop
