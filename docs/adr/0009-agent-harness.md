# 0009 — The agent harness: disposable agents, a durable server

**Status:** Accepted · 2026-10-31 (#41) · amends 0002 and 0003

## Context
Search, research, applying and recruiter replies all need agents that use
tools over several steps, some of them in the background. Today every model
call is one structured completion from a server job. Research into current
harnesses (Claude Agent SDK, OpenAI Agents SDK, OpenHands, Manus, NemoClaw,
Temporal, DBOS, LangGraph) and into OpenShell since v0.1.4 found:

- Every harness runs the same loop: call the model with tools, run the tool
  calls, append the results, repeat. What differs is what wraps it: hard
  caps, typed end states, validation, context trimming, approvals.
- OpenShell has no scheduler (RFC 0004 lists one as a non-goal). Suspend and
  resume exist as an API since v0.1.6, but every in-tree driver answers
  `unimplemented`. NVIDIA's always-on stack (NemoClaw) keeps a sandbox alive
  and loops inside it.
- OpenShell v0.1.0 removed the managed inference router (`inference.local`)
  that ADR 0002 relied on for model calls.
- An inspected endpoint's `enforcement` defaults to `audit` (allow and log)
  and Landlock defaults to `best_effort`.
- Published prompt-injection detectors fall to adaptive attacks 71–100% of
  the time ("The Attacker Moves Second", 2025); only architecture holds.
- Every ATS apply API needs an employer key, so a candidate applies through
  the employer's hosted form, which scores browsers for bots.

## Decision
- **The loop is ours.** `crates/agent` runs one bounded loop per run inside
  the sandbox: limits on steps, wall clock and tokens per run kind, loop
  detection, and one typed end state (`succeeded`, `nothing_new`,
  `needs_you`, `ended_max_steps`, `ended_budget`, `ended_timeout`,
  `ended_loop`, `failed`, `cancelled`). No agent framework dependency.
- **Model calls go through the server.** The agent calls the agent API's
  `complete`, which uses `applicamp-ai` with the run's account. The server
  reserves the call's worst-case cost before forwarding it, so the ledger is
  the one authority on spend. No model key is attached to a sandbox.
- **Tools are typed and tagged** read, draft or outward, fixed for the run
  kind at the start of the run. Tools take ids, not URLs or addresses; the
  server derives every destination. An outward tool only creates an
  approval.
- **The run token carries its capabilities**: run, kind, account, subject,
  the tools it may call with a cap per tool, and an expiry no later than the
  run's deadline.
- **Every sandbox is deleted at the end of its run**, as in 0002. Background
  work is a **standing order** in Postgres: on a schedule or a new inbox item
  it runs a cheap check without a model call and only then starts one bounded
  run in a fresh sandbox. State between runs lives in Postgres and reaches the
  agent through the agent API.
- **Applying is two runs** (amends 0003's paused sandbox): a `prepare` run
  fills a proposal and ends; on accept, a scripted `submit` run with no model
  fills the form again from the approved payload, checks the form still
  matches, and submits or hands it to the user. The server executes only the
  payload whose hash was approved, and accepts each approval once; an expired
  approval means not sent.
- **Policies enforce.** The policy lint requires `enforcement: enforce` on
  every inspected endpoint and `landlock.compatibility: hard_requirement`, and
  runs `openshell-prover`.
- **Single structured calls may stay in server jobs** (search scoring, CV
  drafts, Grill me). Anything that loops over tools or drives a browser runs
  in a sandbox.

## Alternatives
- **Long-lived sandboxes looping inside** (the NemoClaw shape): one injected
  instruction would persist into later runs, the run token could not be
  short-lived, and a shared host caps concurrent sandboxes.
- **A workflow engine** (Temporal, Restate): its primitives (stable ids,
  checkpoints, durable waits, idempotency) are a few tables on the Postgres
  queue we already run.
- **Rig or another Rust agent framework:** pre-1.0 with breaking releases; the
  loop is a few hundred lines.
- **OpenShell provider profiles for model keys:** would keep the key out of
  the sandbox, but spend would no longer pass through our ledger before the
  call.

## Consequences
`applicamp-ai` gains tool calling (`tools`, `tool_calls`, reasoning items
passed through unchanged). The `runs` module owns runs, run events, run
tokens, standing orders and inbox items; an `approvals` module owns
approvals. A run kind is declared by the module that owns its subject (its
tools, policy, limits and output schema); `runs` only executes it, and
results come back through that module's draft tools and the `runs.finished`
event. OpenShell moves to v0.1.6 before production (mandatory gateway auth,
security fixes).
