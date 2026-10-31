# runs Specification

## Purpose
Agents at work: each run of a kind in its own throwaway sandbox with a deny-by-default
network policy, holding no secrets and calling only its kind's tools within their caps; runs
that wait their turn and always end in a state a person can see; and the Agents page, where a
person starts a check and watches what an agent does and what its sandbox allowed and blocked.

## Requirements

### Requirement: Every agent runs in a sandbox that is always removed
Agent work SHALL run only inside a sandbox created for that run, with a
deny-by-default network policy for its kind, and the sandbox SHALL be removed
when the run ends for any reason, including a server restart.

#### Scenario: A run finishes
- **WHEN** a probe run finishes
- **THEN** its sandbox no longer exists and the run shows succeeded with its events

#### Scenario: A run passes its deadline
- **WHEN** a run is still going at its kind's deadline
- **THEN** its sandbox is removed and the run ends `ended_timeout`

#### Scenario: The server restarts mid-run
- **WHEN** the server restarts while a run's sandbox exists
- **THEN** the sweep at startup removes that sandbox and the run ends failed

### Requirement: Every run ends in one state you can see
A run SHALL end in exactly one of `succeeded`, `nothing_new`, `needs_you`,
`ended_max_steps`, `ended_budget`, `ended_timeout`, `ended_loop`, `failed` or
`cancelled`, and SHALL record the tokens and cost it used.

#### Scenario: Stopping a run
- **WHEN** someone stops a run that is going
- **THEN** its sandbox is removed and the run ends `cancelled` with what it had used

### Requirement: Runs wait their turn
An account SHALL have at most one run going at a time and the instance at most
three; other runs SHALL wait queued.

#### Scenario: A second check while one is going
- **WHEN** someone starts a check while another of theirs is running
- **THEN** the second waits queued and starts when the first ends

### Requirement: Sandboxes hold no secrets
A sandbox SHALL receive only its run id, a single-run token and the agent API
address. Model calls SHALL go through the server, which applies the account's
caps and the run's token budget, reserving each call's worst case (its prompt
and its whole answer) before forwarding it.

#### Scenario: A token used for another run
- **WHEN** an agent calls the agent API with its token for a different run, or after its run has finished
- **THEN** the call is answered 404 and nothing changes

#### Scenario: The cap is reached
- **WHEN** an account has used its daily tokens and a run asks for a completion
- **THEN** the call is refused with `ai_daily_limit_reached` and the run records it

#### Scenario: Completions asked at once
- **WHEN** an agent asks for several completions at the same time whose worst cases together pass its run's token budget
- **THEN** only those that fit are forwarded, the others are refused with `run_budget_reached`, and the run's tokens never pass its budget

### Requirement: An agent calls only its kind's tools, within their caps
The run token SHALL name the tools of the run's kind, each with a call cap and
an argument schema. A call to another tool SHALL be answered 404, a call past
its cap SHALL be refused with `tool_cap_reached`, and arguments that do not
match the schema SHALL be refused with `invalid_arguments`.

#### Scenario: A probe calls its tool twice
- **WHEN** a probe calls `probe_echo` a second time past its cap of one
- **THEN** the second call is refused with `tool_cap_reached` and the probe records it

### Requirement: Policies enforce
Every committed policy SHALL deny by default, enforce every rule it inspects,
require its filesystem rules to apply, and never allow LinkedIn, Indeed,
wildcard hosts, IPv4 hosts written other than `a.b.c.d`, private, shared,
loopback, link-local, reserved or NAT64 addresses, or `allowed_ips` ranges wider
than /24 (IPv4) or /48 (IPv6); and every kind's policy SHALL stay within the
committed boundary (`policies/boundary.yaml`), as `openshell-prover` checks.

#### Scenario: A policy that only audits
- **WHEN** a policy file has an endpoint without `enforcement: enforce`
- **THEN** the policy lint fails in CI and names the file and endpoint

### Requirement: You can watch what agents do
The Agents page SHALL list runs and show, live, the agent's own events and
the sandbox's allow/deny log for the selected run, with its sandbox, image,
policy, model, tokens, cost, duration and end state. A run SHALL keep at most
5 000 events; past them, one event says the rest were not kept.

#### Scenario: A run that writes too much
- **WHEN** a run's agent writes more than 5 000 lines
- **THEN** the first 5 000 are kept and shown, then one line says the rest were not kept, and the server's memory does not grow with them

#### Scenario: Watching a check
- **WHEN** someone starts a check from the Agents page
- **THEN** its events appear as they happen, the sandbox log shows a request outside the policy as blocked, and the run ends succeeded
