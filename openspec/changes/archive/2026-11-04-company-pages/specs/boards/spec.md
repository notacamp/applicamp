## ADDED Requirements

### Requirement: Hiring systems are checked once, for every company on them
The registry SHALL hold one row for each hiring system the app reads (Greenhouse, Lever, Ashby,
Workable, Recruitee, Teamtailor and Personio), with the robots rules of the hosts it reads, its
terms read by the model once (again only when its terms change) and its `Content-Signal`. The app
SHALL approve a system by itself, recording "approved by the app", when every path it reads is
allowed, its terms allow apps or say nothing about automated reading and it asks nothing of
artificial intelligence tools. A system whose content signal says `ai-train=no` SHALL wait for the
owner with the blocker `needs_choice`, saying that reading its pages is allowed and that it asks
artificial intelligence tools not to learn from them, and SHALL NOT be approved by the app;
approving it SHALL be the owner's choice and SHALL work although the blocker is listed. A system
whose rules forbid reading, or whose terms forbid it or ask first, SHALL wait with its own blocker.
SmartRecruiters SHALL NOT be a hiring system: its robots.txt allows only LinkedInBot.

#### Scenario: A clean system
- **WHEN** a system's robots rules allow its reader's paths, its terms say nothing about automated reading and it has no content signal against learning
- **THEN** the app approves it, and its row says it was approved by the app

#### Scenario: Lever's signal
- **WHEN** Lever's robots.txt carries `ai-train=no`
- **THEN** its row waits for the owner with `needs_choice`, it is approvable, the app does not approve it, and its company pages are not read until the owner approves it

#### Scenario: Terms that ask first
- **WHEN** a system's terms say automated reading needs written permission
- **THEN** it waits with `needs_permission` and cannot be approved until the owner clears it

### Requirement: A company page is found through apply links, with no request
Every job read (by a search, a company watch or a board's check) SHALL have its links (its own,
its apply link and the links it was seen on) matched against the hiring systems' public addresses,
and each new company SHALL be added to the registry as a candidate company page: system plus
company unique together, with its name, the page where its jobs are and how it was found ("an apply
link on <site>", "web search", "your company watch"). Matching and adding SHALL make no request and
call no model. A page registered for a declined system SHALL be declined, with the reason.

#### Scenario: An apply link on Lever
- **WHEN** a board's job says to apply at `https://jobs.lever.co/acme/0b6d…`
- **THEN** a candidate company page "Acme" on Lever is added once, however many jobs name it

#### Scenario: A link that is not a hiring system
- **WHEN** a job's links are on a company's own site
- **THEN** no company page is added

### Requirement: A company page is approved by the app after one sample read
The app SHALL approve a company page, recording "approved by the app", when its hiring system is
approved, a sample read through the system's reader returns jobs and one of them was posted in the
last 60 days. It SHALL make no terms call and no model call per company. A page whose system waits
SHALL stay waiting, and SHALL NOT be approvable before its system; a page whose system the owner
declined SHALL be declined with it. A page that shows no recent job SHALL wait and be read again a
month later; one that did not answer, a week later.

#### Scenario: Recent jobs
- **WHEN** a company page's system is approved and its sample read lists a job posted three weeks ago
- **THEN** the app approves the page

#### Scenario: Nothing recent
- **WHEN** its newest job was posted four months ago
- **THEN** the page waits, says "no job was posted in the last 60 days", and is not read again for 30 days

#### Scenario: The system waits
- **WHEN** a company page's system waits for the owner
- **THEN** the page is not read and the owner cannot approve it before the system

#### Scenario: The system is declined
- **WHEN** the owner declines a hiring system
- **THEN** its company pages are declined with "Its hiring system was declined." and so are the ones found afterwards

### Requirement: Company pages are checked daily within a limit
A daily run SHALL check the hiring systems that are due, then at most
`APPLICAMP_SOURCES__COMPANY_PAGE_CHECKS` new company pages a day (300), taken in turns across the
approved systems, each host kept to the instance's request pace; a system that asks to slow down
SHALL be left alone for the day, and one whose robots rules now forbid reading SHALL have no page
read.

#### Scenario: The limit
- **WHEN** 500 company pages wait and the limit is 300
- **THEN** 300 are checked that day, however many runs it has, and the rest wait for the next day

### Requirement: Searches do not read company pages yet
A search SHALL read company pages only through the companies the person watches, as before; with
`APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES` off (the default) it SHALL NOT read the registry's company
pages, whatever their number, until a job store exists.

#### Scenario: Thousands of company pages
- **WHEN** thousands of company pages are approved
- **THEN** a search reads the same boards as before them

### Requirement: Company pages can be found by a web search within a cost limit
With `APPLICAMP_SEARCH__BRAVE_API_KEY_FILE` set, the app SHALL run a web search every week: at most
50 queries (configurable), built from the roles of every account's goals and the approved hiring
systems' hosts, never more than 1000 in a calendar month (configurable), each query counted
whether it was answered or not, a refused key or limit stopping the run. Each result link on a
hiring system's public address SHALL only register a candidate company page; nothing else of what
the search answers SHALL be kept. Without the key the search SHALL be off and the Boards list SHALL
say so.

#### Scenario: The monthly limit
- **WHEN** 998 queries were asked this month and the weekly run wants 50
- **THEN** it asks 2

#### Scenario: No key
- **WHEN** no key file is set
- **THEN** nothing is asked and the Boards page says the web search is off

### Requirement: Hiring systems and company pages show by their kind
The Boards list SHALL show a hiring system and a company page with their kind in plain words
("Hiring system", "Company page") and, for a page, the system it hires through ("Hires through
Lever"), in the person's language, never saying "ATS", "robots" or "API". Each board of the list
SHALL carry its kind, system and company. The list SHALL show every board and system and at most 100
company pages (the waiting ones first), with the counts of all company pages by where they stand and
the state of the web search.

#### Scenario: A company page on Lever
- **WHEN** the list holds the approved page of Acme on Lever
- **THEN** its row says "Company page" and "Hires through Lever"
