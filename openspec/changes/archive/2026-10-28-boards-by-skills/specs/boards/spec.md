## ADDED Requirements

### Requirement: Boards for your stack are suggested, never decided
The seed list SHALL include researched boards built around common stacks (Python, PHP/Laravel, Go,
Ruby, JavaScript) as candidates only: a real config when the board has a feed, otherwise none,
and a blocker only for a fact quoted from its terms or robots rules with its source. The app SHALL
NOT approve or decline them.

#### Scenario: A stack board with a feed
- **WHEN** the server starts with LaraJobs in the seed list
- **THEN** it waits for the owner as a candidate with its feed config, is checked at start, and shows its sample jobs

#### Scenario: A stack board without a feed
- **WHEN** Ruby on Remote answers a program with a challenge
- **THEN** it waits as a candidate with no feed address, its note says so, and Check looks for one

### Requirement: Boards say how well they fit your skills
The Boards page and a board's peek SHALL show, next to the salary, "fits your skills N%": the share
of the board's listed engineering jobs (a candidate's check sample included) whose title or
description names one of the person's top 3 skills, with how many jobs it is from. It SHALL be
absent when the profile lists no skills or the board has no listed jobs. The order of the boards
SHALL stay the average salary.

#### Scenario: A Ruby developer opens the list
- **WHEN** a board's sample has 10 engineering jobs and 4 name Ruby, Rails or React
- **THEN** its row says "fits your skills 40%" beside its salary, and the order is unchanged

#### Scenario: No skills
- **WHEN** the profile lists no skills
- **THEN** no board shows the label
