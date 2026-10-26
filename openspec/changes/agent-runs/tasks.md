# Tasks
## 1. Server
- [ ] 1.1 `runs` module: schema, lifecycle, queue + concurrency, time limits, startup sweep, cancel
- [ ] 1.2 SandboxRuntime trait + fake runtime; OpenShell runtime behind the trait (templates from policies, log stream, delete, sweep)
- [ ] 1.3 Agent API with run tokens (context, events, complete via crates/ai, finish), rate limit, tool allow-list per kind
- [ ] 1.4 OIDC issuer for the gateway (discovery, JWKS, self-minted tokens) — nothing else issued
- [ ] 1.5 Event retention job, metrics
## 2. Agent
- [ ] 2.1 `crates/agent` runner + `probe` kind; agent image in publish.yml
- [ ] 2.2 `policies/probe.yaml` + policy lint in CI
## 3. Client
- [ ] 3.1 Agents page: run list, live console (SSE), sandbox log, run facts, Run a check
## 4. Finish
- [ ] 4.1 Tests: lifecycle with the fake runtime (success, failure, timeout, cancel, sweep), token checks (wrong run, finished run, expired), cap enforcement; e2e: Run a check → succeeded with events visible (desktop + phone)
- [ ] 4.2 OpenAPI + client types regenerated, changelog entry file, gates green in CI, recording at 375px and desktop
