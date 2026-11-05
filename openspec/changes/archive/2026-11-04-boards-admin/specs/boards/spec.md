## ADDED Requirements

### Requirement: Board owners see every board's status on one page
The Job sites page SHALL show a board owner the totals of the boards (being read as job sites and as companies, waiting for the owner,
being checked, not allowed, paused), what happened in the last 7 days from what the app recorded (boards found, checked, read again,
approved), each board waiting for the owner with its reason in plain words, and every board in a table with a search and filters by kind
(job site or company page), status and kind of jobs, each opening a side panel. A figure the app has no record of SHALL be left out, never
estimated. Anyone who is not a board owner SHALL keep the page they have, and the admin endpoints SHALL answer them 404.

#### Scenario: An owner opens the page
- **WHEN** a board owner opens Job sites
- **THEN** they see the totals, this week, the boards waiting for them and all boards, and a board's panel shows its decision first, then its facts and each check in plain words with the quoted sentence

#### Scenario: Someone who is not an owner
- **WHEN** a person who does not own the boards opens Job sites, or asks for the admin overview
- **THEN** they keep the page they had, and the overview answers 404

### Requirement: A board that needs permission has its email drafted and its answer recorded
For a board whose rules ask for written permission, the app SHALL draft the email (the board's name, what Applicamp reads, a link back, at most
one read an hour, no reposting, closed jobs dropped, addressed to the board's contact when its check found one) and SHALL NOT send it. The owner
SHALL be able to record that they sent it, which stores the date, and then record the answer: a yes approves the board through the approve path
and keeps "They said yes on <date>", a no declines it through the decline path with "They said no on <date>". An answer SHALL NOT be recorded
before the request was asked, and a yes SHALL NOT override robots rules that forbid the feed or a key the board still needs.

#### Scenario: Sent, then yes
- **WHEN** the owner copies the email, sends it, presses "I sent it" and later "They said yes"
- **THEN** the date they sent it is kept, the board is approved and searches read it

#### Scenario: They said no
- **WHEN** the owner records "They said no"
- **THEN** the board is declined with "They said no on <date>"

#### Scenario: A yes the robots rules still refuse
- **WHEN** the owner records a yes for a board whose robots rules forbid its feed
- **THEN** the answer is refused naming the rule, nothing is recorded, and the request stays open

### Requirement: A decline can be taken back
A board the owner declined from the Job sites page SHALL stay in its row, showing "Declined" with Undo, until the page is reloaded, and Undo SHALL put the board back where it was: read again when it had been approved, else waiting for the owner.

#### Scenario: They said no, then undo
- **WHEN** the owner records "They said no" and then presses Undo
- **THEN** the board is waiting for the owner again with no decline reason
