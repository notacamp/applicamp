## RENAMED Requirements
- FROM: `### Requirement: New job boards are found and approved by a person`
- TO: `### Requirement: New job boards are found, checked and approved by the app when the check is clean`

## MODIFIED Requirements

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

## ADDED Requirements

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
