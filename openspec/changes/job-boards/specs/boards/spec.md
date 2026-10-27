## ADDED Requirements

### Requirement: New job boards are found and approved by a person
The app SHALL look for new job boards every week, check each one's robots
rules, terms and machine-readable feed automatically, and SHALL NOT read a
board's jobs until the owner approves it after seeing that check.

#### Scenario: A board found by discovery
- **WHEN** discovery finds a board with a public JSON feed and terms that mention automated access
- **THEN** it waits on the Boards page with the endpoint, the matched terms sentences and sample jobs, and no search reads it until it is approved

#### Scenario: A board whose robots rules forbid the feed
- **WHEN** a candidate's robots.txt disallows its API path
- **THEN** the check says so and the board cannot be approved without editing its config to an allowed path
