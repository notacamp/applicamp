## ADDED Requirements

### Requirement: The nightly search recurs every night
Each nightly search SHALL schedule the next night's while it runs, whether or not the server
restarts in between, and there SHALL be at most one waiting at a time. The weekly board
discovery SHALL recur the same way.

#### Scenario: A night with no restart
- **WHEN** a nightly search runs and the server is not restarted before the next evening
- **THEN** the next evening's nightly search runs

#### Scenario: The server starts while one is waiting
- **WHEN** the server starts and a nightly search is already scheduled
- **THEN** no second one is scheduled
