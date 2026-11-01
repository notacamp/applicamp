## ADDED Requirements

### Requirement: Each kind's sandbox template is one the gateway takes
Each run kind SHALL have its sandbox template on the gateway under a name the
gateway accepts: a DNS label of at most 19 characters, distinct for every kind.
When the templates cannot be made at server start (the gateway is not up yet),
the server SHALL try again until the gateway takes them, without a restart.

#### Scenario: A kind with a long name
- **WHEN** a kind's name is longer than a template name may be
- **THEN** its template name is at most 19 characters, a DNS label, and differs from every other kind's

#### Scenario: The gateway comes up after the server
- **WHEN** the gateway is unreachable at server start and comes up later
- **THEN** the templates are made within a minute and runs start without restarting the server
