# boards Specification

## Purpose
The job boards a search reads: a registry of boards, each read within its terms and credited
in the app, new ones found every day and checked automatically, none read before it is approved (by the
app when its check is clean, else by a person), all ranked by the salaries they list; and a view of what every search and check
scanned on each board.

## Requirements

### Requirement: New job boards are found, checked and approved by the app when the check is clean
The app SHALL look for new job boards every day on public lists, check each one's robots rules,
terms and machine-readable feed automatically, and SHALL approve a board by itself, recording
"approved by the app" with the date, only when its check is clean: its config reads sample jobs,
its robots rules allow every path read, at least 5 of the sample items look like job postings (a
title and a company or an apply link), it posted jobs in the last 30 days, its terms allow apps
or say nothing about automated reading, it has no blocker and it needs no key. A board the owner
paused, declined or edited SHALL never be approved by the app. Only the owner SHALL decline a
board: the app SHALL NOT decline one on the owner's behalf, and what its research found SHALL be
shown as a note (an opinion) or as a blocker (a fact: terms that forbid automated reading, robots
rules that disallow the job pages, a need for permission), never as a decline.

#### Scenario: A board found by discovery with a clean check
- **WHEN** discovery finds a board with a feed of at least 5 postings from the last 30 days, allowed by its robots rules, with terms that say nothing about automated reading
- **THEN** the app approves it, its peek says "Approved by the app on" the date, and searches read it

#### Scenario: Not a job board
- **WHEN** a found site's sample has fewer than 5 items with a title and a company or an apply link
- **THEN** it waits for the owner with its check, and the app does not approve it

#### Scenario: A board whose robots rules forbid the feed
- **WHEN** a candidate's robots.txt disallows its API path
- **THEN** the check says so and the board cannot be approved without editing its config to an allowed path

#### Scenario: A board the owner never declined is never shown as declined
- **WHEN** a board was declined by the seed list rather than by the owner (Landing.jobs, The Muse, Wellfound, Dice)
- **THEN** it waits for the owner as a candidate, with the research as its note, and the Declined section lists only boards the owner declined

#### Scenario: Terms that forbid automated reading
- **WHEN** a waiting board's terms forbid automated reading (Landing.jobs)
- **THEN** its peek quotes the clause with a link to the terms, Approve stays disabled, and its site is not read by the checks at start

#### Scenario: A board the owner edited
- **WHEN** the owner edited a candidate's config and a check finds it clean
- **THEN** the app does not approve it

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

### Requirement: A board may give the company's own website
A job board's field mapping MAY name where it gives the hiring company's
website. When a search adds a job from that board, the website SHALL be kept
on the company (never replacing one the person typed), so the company's own
pages can be read for a tailored CV without asking.

#### Scenario: The board gives the website
- **WHEN** a board lists a job with the company's website and a search adds it
- **THEN** the company's site is that website, and Prepare does not ask for it

### Requirement: A board's terms are read by a model
The app SHALL read each board's terms with one model call (the `complete` role) from the terms
sentences the check picked, once per board and again only when those sentences change, charged to
the instance's AI budget and refused, never retried, past a spending limit. The call SHALL return
a verdict (allow, silent, forbid, ask first or personal only) and the sentence that decided it,
which SHALL be one of the sentences it was given. The terms text SHALL be handled as data, never
as instructions. A board with no terms page SHALL be silent. A forbid SHALL be a `terms_forbid`
blocker; ask first, personal only, and a forbid that the site's own robots rules contradict with
an explicit `Allow` for the feed SHALL be a `needs_permission` blocker showing the quote.

#### Scenario: Terms that ask for permission
- **WHEN** a board's terms say automated access needs written permission
- **THEN** the board waits for the owner with a "needs permission" reason quoting that sentence, and Approve stays disabled

#### Scenario: A spending limit
- **WHEN** the instance's AI spending limit is reached during discovery
- **THEN** no call is sent, the run stops cleanly, the board waits with a note that its terms are not read yet, and nothing is approved by the app

#### Scenario: A quote nobody wrote
- **WHEN** the model forbids reading with a sentence that is not in the terms it was given
- **THEN** the answer is discarded and the board is not approved by the app

### Requirement: Boards are tagged by what they offer
The app SHALL tag each board, in the same call that reads its terms, with the kinds of jobs its
sample shows (from a fixed list), where its jobs are, whether they are mostly remote, and the
specialities it is built around.

#### Scenario: A remote design board
- **WHEN** a board's sample jobs are remote design roles open worldwide
- **THEN** its tags include `design`, `worldwide` and `remote`

### Requirement: The robots Content-Signal is recorded
The app SHALL parse a robots.txt `Content-Signal` line and keep and show its values. A signal
such as `ai-train=no` SHALL NOT stop a board from being read, and the app SHALL NOT train on jobs.

#### Scenario: ai-train is no
- **WHEN** a board's robots.txt says `Content-Signal: search=yes, ai-train=no`
- **THEN** the board is read as usual, and its peek says the site asks that its content is not used to train models

### Requirement: Discovery reads public lists of boards
Discovery SHALL read the CC0 and MIT lists of job boards named in its source table (awesome-job-boards,
awesome-remote-job, remote-working-list, awesome-remote-jobs, idimetrix's awesome-job-boards and
Wikidata's employment websites), record on each board it adds which list it was found through, check
at most 100 boards a run (configurable), and never add sites on its never list (LinkedIn, Indeed,
Glassdoor and the like).

#### Scenario: A list names a feed
- **WHEN** a list gives a board's feed address
- **THEN** the board is added with that feed as its endpoint and its check reads it first

### Requirement: Boards that need permission, and the research's seeds
The seed list SHALL include the boards researched on 2026-11-04 as candidates with real configs, so
their check approves them when clean, and SHALL state `needs_permission` with its quote and source
for boards whose terms or robots rules require asking first (Jobgether, VueJobs, Elixir Forum). A
feed whose XML is malformed (a bare `&`, an unclosed item, tags in capitals, a byte-order mark)
SHALL still be read.

#### Scenario: A malformed feed
- **WHEN** a feed has an unescaped `&` in a link and an item that is never closed
- **THEN** its items are read
