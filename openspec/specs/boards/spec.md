# boards Specification

## Purpose
The job boards a search reads: a registry of boards, each read within its terms and credited
in the app, new ones found every week and checked automatically, none read before a person
approves it, all ranked by the salaries they list; and a view of what every search and check
scanned on each board.

## Requirements

### Requirement: New job boards are found and approved by a person
The app SHALL look for new job boards every week, check each one's robots
rules, terms and machine-readable feed automatically, and SHALL NOT read a
board's jobs until the owner approves it after seeing that check. Only the
owner SHALL decline a board: the app SHALL NOT decline one on the owner's
behalf, and what its research found SHALL be shown as a note (an opinion) or
as a blocker (a fact: terms that forbid automated reading, robots rules that
disallow the job pages), never as a decline.

#### Scenario: A board found by discovery
- **WHEN** discovery finds a board with a public JSON feed and terms that mention automated access
- **THEN** it waits on the Boards page with the endpoint, the matched terms sentences and sample jobs, and no search reads it until it is approved

#### Scenario: A board whose robots rules forbid the feed
- **WHEN** a candidate's robots.txt disallows its API path
- **THEN** the check says so and the board cannot be approved without editing its config to an allowed path

#### Scenario: A board the owner never declined is never shown as declined
- **WHEN** a board was declined by the seed list rather than by the owner (Landing.jobs, The Muse, Wellfound, Dice)
- **THEN** it waits for the owner as a candidate, with the research as its note, and the Declined section lists only boards the owner declined

#### Scenario: Terms that forbid automated reading
- **WHEN** a waiting board's terms forbid automated reading (Landing.jobs)
- **THEN** its peek quotes the clause with a link to the terms, Approve stays disabled, and its site is not read by the checks at start

### Requirement: Boards are ranked by average salary
The Boards page and the approval queue SHALL list boards by the average stated
salary of their relevant jobs, highest first, showing how many jobs the
average comes from and what share state a salary; boards with no salary data
SHALL come last.

#### Scenario: Two boards
- **WHEN** one board's relevant jobs average €120k a year and another's €80k
- **THEN** the first is listed above the second, each with the number of jobs behind its average

#### Scenario: Country boards with few salaries
- **WHEN** a Swiss board and a Portuguese board each list fewer than 10 salaries
- **THEN** they are ranked by their countries' typical engineering pay, the Swiss one first, each marked as a country benchmark

### Requirement: See what was scanned
The Boards page SHALL show, for each search and discovery run, every board
that was scanned with how many jobs were read, passed the person's
requirements, were scored by AI and matched, what the AI cost, and why a
board was skipped or failed.

#### Scenario: After the nightly search
- **WHEN** the nightly search has run
- **THEN** the Scanned view lists it with each board's read, passed, scored and matched counts and its AI cost, and a board skipped for its polling limit says so

### Requirement: Every waiting board can be approved or says why
A board waiting for the owner SHALL either be approvable or show every
reason it is not, in order, each with what exactly (the terms' sentence, the
robots rule, the field, how to get a key): its terms forbid automated
reading, its robots rules disallow it, it needs an API key, it has no feed
address yet, its config does not read, it was not checked yet. A reason the
client does not know SHALL still be shown, by its detail. Seeded boards
waiting for the owner SHALL be checked at start once with their config (one
request per path, robots respected), unless a fact forbids reading them.

#### Scenario: A seeded board after its check
- **WHEN** the server starts with SwissDevJobs waiting and never checked
- **THEN** it is checked once with its own config, sample jobs appear, and Approve is enabled

#### Scenario: A board with no feed address
- **WHEN** a waiting board has no config that reads
- **THEN** its peek says it has no feed address yet and that Check looks for one

### Requirement: Boards that need an API key
A board whose API needs a key SHALL say "Needs an API key from <board>:
<how to get it>" with a link, and SHALL NOT be asked for jobs without it.
Only a board owner SHALL set or remove the key; it SHALL be stored sealed
with the instance secret, sent only as the parameters the config names to
the host it was set for, and never returned to a client, logged, cached by
address or kept in a report (the peek says only "Key set").

#### Scenario: ITJobs.pt without a key
- **WHEN** the owner opens ITJobs.pt
- **THEN** it says it needs an API key from ITJobs.pt and how to get one, Approve is disabled, and its API was never asked

#### Scenario: A key set
- **WHEN** the owner pastes the key and checks the board again
- **THEN** the peek says "Key set" without showing it, the check reads sample jobs with it, and Approve is enabled

### Requirement: Errors show where you act
Every action on a board (approve, pause, check, decline, save the config, set
the key, use a proposal) SHALL show its error right beside its button, with
the app's message and the server's own explanation. A config that does not
read SHALL be refused naming the field and what is wrong.

#### Scenario: A bad config edit
- **WHEN** the owner saves a config whose `max_pages` is `"five"`
- **THEN** the error beside Save names `max_pages` and says what is wrong

### Requirement: The check proposes a config
When a board has no usable config, its check SHALL probe the site's RSS
links, `/api`, `/jobs/api`, `/feed`, `/rss` and JSON-LD postings through its
sitemap (robots respected, one request per path), and SHALL propose what
reads as an endpoint and a field mapping, shown as each field and where it is
read from, with "Use this config". The raw JSON config SHALL stay available
under "Advanced". A proposal SHALL apply by itself only to a discovered or
suggested board nobody edited.

#### Scenario: A site with an RSS link
- **WHEN** a waiting board with no config has a homepage linking its RSS feed
- **THEN** its check proposes that feed with title, link and description mapped, and "Use this config" makes it approvable

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
