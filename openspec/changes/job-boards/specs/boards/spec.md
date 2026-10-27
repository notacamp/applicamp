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

### Requirement: Boards are ranked by average salary
The Boards page and the approval queue SHALL list boards by the average stated
salary of their relevant jobs, highest first, showing how many jobs the
average comes from and what share state a salary; boards with no salary data
SHALL come last.

#### Scenario: Two boards
- **WHEN** one board's relevant jobs average €120k a year and another's €80k
- **THEN** the first is listed above the second, each with the number of jobs behind its average
