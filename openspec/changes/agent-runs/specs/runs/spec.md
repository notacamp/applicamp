## ADDED Requirements

### Requirement: Every agent runs in a sandbox that is always removed
Agent work SHALL run only inside a sandbox created for that run, with a
deny-by-default network policy for its kind, and the sandbox SHALL be removed
when the run ends for any reason, including a server restart.

#### Scenario: A run finishes
- **WHEN** a probe run finishes
- **THEN** its sandbox no longer exists and the run shows succeeded with its events

#### Scenario: The server restarts mid-run
- **WHEN** the server restarts while a run's sandbox exists
- **THEN** the sweep at startup removes that sandbox and the run is marked failed

### Requirement: Sandboxes hold no secrets
A sandbox SHALL receive only its run id, a single-run token and the agent API
address. Model calls SHALL go through the server, which adds the key and
applies the account's daily cap.

#### Scenario: A token used for another run
- **WHEN** an agent calls the agent API with its token for a different run, or after its run has finished
- **THEN** the call is answered 404 and nothing changes

#### Scenario: The cap is reached
- **WHEN** an account has used its daily tokens and a run asks for a completion
- **THEN** the call is refused with `ai_cap_reached` and the run records it

### Requirement: You can watch what agents do
The Agents page SHALL list runs and show, live, the agent's own events and
the sandbox's allow/deny log for the selected run, with its sandbox, image,
policy, model, tokens and duration.

#### Scenario: Watching a check
- **WHEN** someone starts a check from the Agents page
- **THEN** its events appear as they happen, the sandbox log shows a request outside the policy as blocked, and the run ends succeeded
