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
its robots rules allow every path read, at least 5 of the sample items are postings (a title and a
company, an apply link, or a link to the job's own page on the site), the jobs do not all apply on
one company's hiring page, it is alive (a job posted in the last 30 days, or, when no job says when,
the feed's own date or the date the site answered with, that recent), its terms allow apps or say
nothing about automated reading, it has no blocker and it needs no key. A board the owner paused,
declined or edited SHALL never be approved by the app. Only the owner SHALL decline a board: the app
SHALL NOT decline one on the owner's behalf, and what its research found SHALL be shown as a note (an
opinion) or as a blocker (a fact: terms that forbid automated reading, robots rules that disallow the
job pages, a need for permission), never as a decline.

#### Scenario: A board found by discovery with a clean check
- **WHEN** discovery finds a board with a feed of at least 5 postings from the last 30 days, allowed by its robots rules, with terms that say nothing about automated reading
- **THEN** the app approves it, its peek says "Approved by the app on" the date, and searches read it

#### Scenario: Not a job board
- **WHEN** a found site's sample has fewer than 5 items that are postings
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
The Searches tab of the Job sites page SHALL show each search with four
numbers first: jobs read, read in full, good matches and its cost. Under the
latest search it SHALL show how many sites and company pages the job store gave
jobs from, where the matches came from (each match under one source,
a job site or a company's hiring page shown by the company's name, the five biggest
one by one and the rest as one line each for company pages and for sites, adding
up to the good matches), and why the
other jobs were left out, in plain words with counts that add up to the jobs
read (including the jobs not among the closest to the person's roles, which got
no quick look), and how many of the jobs read in full do not say where they hire and
were kept with a note. What the search did on each site (jobs read, quick
look, read in full, good matches, cost, and why a site was skipped, failed or
read in part) SHALL be behind "Show by site". Earlier searches SHALL be one
row each with their four numbers and Details; a discovery run SHALL be one
row with what it checked under Details. Search now SHALL be a plain button
with the estimated cost on the line under it and no keyboard shortcut shown.

#### Scenario: After the nightly search
- **WHEN** the nightly search has run
- **THEN** the Searches tab shows it first with the jobs read, read in full, good matches and cost, and under "Show by site" each site's counts and its cost, a site that failed saying so

#### Scenario: The counts add up
- **WHEN** a search read 1,112 jobs and found 6 good matches
- **THEN** the reasons under "Why jobs were left out" add up to 1,106

#### Scenario: Read from the store
- **WHEN** a search drew jobs from 4 sites and 212 company pages
- **THEN** the tab says it read from 4 sites and 212 company pages

#### Scenario: A site read in part
- **WHEN** a site's third page did not answer during a search
- **THEN** its line under "Show by site" says so, and its first two pages count as read

#### Scenario: Where the matches came from
- **WHEN** a search found 16 good matches, 9 credited to Himalayas, 5 to Wayfarer Co's page and 2 to Remote OK
- **THEN** the tab lists "Himalayas · 9", "Wayfarer Co’s hiring page · 5" and "Remote OK · 2", adding up to 16

#### Scenario: Many company pages
- **WHEN** twelve company pages gave one or two matches each
- **THEN** the list shows the five biggest sources and then "N company hiring pages" as one line with their total

### Requirement: Every waiting board can be approved or says why
A board waiting for the owner SHALL either be approvable or show every
reason it is not, in order, each with what exactly (the terms' sentence, the
robots rule, the field, how to get a key): its terms forbid automated
reading, its robots rules disallow it, it needs an API key, it has no feed
address yet, its config does not read, it was not checked yet. A reason the
client does not know SHALL still be shown, by its detail. Seeded boards
waiting for the owner SHALL be checked at start once with their config (one
request per path, robots respected), unless a fact forbids reading them.
A candidate the check did not approve SHALL also carry, in its report, the reason it still waits
as one plain sentence that does not use the words RSS, robots or API.

#### Scenario: A seeded board after its check
- **WHEN** the server starts with SwissDevJobs waiting and never checked
- **THEN** it is checked once with its own config, sample jobs appear, and Approve is enabled

#### Scenario: A board with no feed address
- **WHEN** a waiting board has no config that reads
- **THEN** its peek says it has no feed address yet and that Check looks for one

#### Scenario: The reason in a sentence
- **WHEN** a check found a feed whose newest job is from three months ago
- **THEN** the report says "Its newest job is from" that date "more than 30 days ago.", and an approved board's report says nothing

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
When a board has no usable config, its check SHALL look for the site's list of jobs, and SHALL
propose what reads as an endpoint and a field mapping, shown as each field and where it is read
from, with "Use this config". The raw JSON config SHALL stay available under "Advanced". A proposal
SHALL apply by itself only to a discovered or suggested board nobody edited. A proposed feed config
SHALL map the item dates (the element most items carry among `pubDate`, `published`, `dc:date` and
`updated`). Where it looks is in the requirement "The check looks for a site's list of jobs in more
places, within a request budget".

#### Scenario: A site with an RSS link
- **WHEN** a waiting board with no config has a homepage linking its RSS feed
- **THEN** its check proposes that feed with title, link, description and date mapped, and "Use this config" makes it approvable

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

### Requirement: Approved sites are read on their own schedule into a job store
The app SHALL read every approved job board, and every company page whose hiring system is
approved and whose robots rules allow reading, on its own schedule, and keep the jobs they list in
one store with, for each job, the site it came from, its reference there, its title, company, links,
place, how it is worked, pay, dates and description (cut at a limit), when the app first and last
saw it and when it closed. Only approved rows SHALL be read; a paused, declined or waiting site, a
company page whose system waits, and a site `APPLICAMP_SOURCES__ENABLED` leaves out SHALL NOT. The
app SHALL call no model while reading, and SHALL feed every read's apply links to the registration
of company pages, as a search's read does. A search SHALL still read boards live until it reads the
store.

#### Scenario: A board approved by the app
- **WHEN** a job board is approved
- **THEN** the next reading reads it without anyone searching, and its jobs are in the store

#### Scenario: A waiting or paused site
- **WHEN** a board waits for a decision, or an approved board is paused
- **THEN** it is not read for the store, and its stored jobs are not offered to a search

#### Scenario: A company page
- **WHEN** a company page and its hiring system are approved
- **THEN** its jobs are read through the system's reader into the store, once a day at most

### Requirement: A site's own limits come first
The app SHALL read a site no oftener than its polling interval (counting searches that read it
live), its terms' fetches a day, the `Retry-After` it gave and the instance's least interval between
two reads; a company page at most once a day. It SHALL keep to one request at a time on a host, at
the instance's pace between two requests to it, and start at most a set number of reads a minute
over all hosts. A site that answered 429 or 503 SHALL be left alone for as long as it said, an hour
if it said nothing.

#### Scenario: Fetches a day
- **WHEN** a board's terms allow two fetches a day
- **THEN** it is read every twelve hours at the soonest

#### Scenario: A search read it lately
- **WHEN** a search read a board live five minutes ago and its polling interval is fifteen minutes
- **THEN** the store does not read it until the interval has passed

#### Scenario: Asked to slow down
- **WHEN** a board answers 429 with `Retry-After: 7200`
- **THEN** it is not read again for two hours, and its line in the log of reads says it was rate limited

### Requirement: Only new or changed jobs are kept, and a site is asked whether it changed
The app SHALL write a job only when it is new or changed since the last read, and touch the ones it
sees again. It SHALL send the `ETag` and `Last-Modified` a site gave as a conditional request
where the reader's answer is one list, and when the site answers that nothing changed SHALL keep
every job and parse nothing.

#### Scenario: Nothing changed
- **WHEN** a site answers 304 to a conditional request
- **THEN** its jobs stay open as they are, the read is logged as not modified, and the site is seen again

#### Scenario: One job changed
- **WHEN** a read lists a job with a new title, a job it had, and a job it did not
- **THEN** one job is rewritten, one is added, and the other is only seen again

### Requirement: Jobs a site no longer lists are closed, then deleted
When a read lists everything a site has, the app SHALL close the stored jobs it no longer lists, and
reopen a job the site lists again. A read stopped by a page limit, a read through a sitemap, and a
read that would close most of a site's jobs at once SHALL close nothing. The app SHALL delete a
closed job after a number of days (14 by default), drop a job past its expiry (for a site whose
terms say to drop expired jobs) and delete a job no read has seen for 30 days, and SHALL keep a
limited number of jobs per site and in all, the newest.

#### Scenario: A job the board dropped
- **WHEN** a full read no longer lists a job
- **THEN** it is closed, not offered to a search, and deleted fourteen days later

#### Scenario: A partial answer
- **WHEN** a read stops at its page limit with more pages announced
- **THEN** the jobs it did not list stay open

#### Scenario: An expired job
- **WHEN** a board whose terms say to drop expired jobs lists a job past its expiry
- **THEN** the job is not stored

### Requirement: A quiet site is read less often
The app SHALL read a site less often as its reads bring nothing new (twice as far apart after three
reads in a row with nothing new, then four, eight and sixteen times, up to a day, a week for a
company page), and SHALL return to its own interval when a read brings something, never reading
oftener than its limits allow.

#### Scenario: Nothing new
- **WHEN** a board's third read in a row brings nothing new
- **THEN** its next read is twice as far away as before

### Requirement: Searches can read the store, and the owner sees how it goes
The module SHALL offer a read of the open jobs of approved sites filtered on posted since, work
mode, regions and the words of someone's roles and skills, with a limit, giving jobs as the readers
give them. `GET /api/v1/boards/store` SHALL tell the board owners how many jobs are stored, how many
are new in the last 7 days, how many sites were read today and how many reads failed, and SHALL
refuse anyone else.

#### Scenario: The owner's totals
- **WHEN** a board owner asks for the store's totals after a reading
- **THEN** the answer counts the jobs stored, those new in 7 days, the sites read today and the failures

#### Scenario: Someone else
- **WHEN** an account that does not manage the boards asks
- **THEN** it is refused

### Requirement: The Workable list adds every Workable company that is hiring, once a day
The app SHALL read Workable's documented list of every company's jobs
(`https://www.workable.com/boards/workable.xml`) at most once a day, as a stream: it SHALL parse
the list while it downloads and never hold its body in memory, SHALL check robots.txt for the
list's path before each download, SHALL send `If-None-Match` and `If-Modified-Since` from the last
complete download and treat `304` as unchanged, SHALL honour `429` and `503` with `Retry-After`,
and SHALL stop at a configured byte cap and time limit. A download that is cut or interrupted
SHALL keep the counts of what it processed, and SHALL NOT be the one the next request is
conditional on. The list SHALL be used only to grow the registry: no model SHALL be called, and the
jobs themselves SHALL NOT be stored, only how many jobs the list had for each company. Job links
SHALL be kept exactly as the list gives them. A company on the configured exclusion list (boards
Workable already integrates with, empty by default) SHALL never be added from the list.

#### Scenario: The first read
- **WHEN** the list is read for the first time
- **THEN** each company with a job posted in the last 60 days is counted with its job count, and a company with only old jobs is not added

#### Scenario: Not modified
- **WHEN** the server answers `304` to the conditional request
- **THEN** nothing is downloaded, the validators are kept, and the pages already added stay

#### Scenario: Cut by the byte cap
- **WHEN** the download reaches its byte cap
- **THEN** the companies counted so far are kept, the outcome is recorded as capped, and the next request is not conditional

#### Scenario: Asked to wait
- **WHEN** the server answers `429` with `Retry-After`
- **THEN** no download is made before that time (an hour at least, a day at most)

### Requirement: Listed companies become Workable company pages that the app approves with their system
For each company the list names with a job posted in the last 60 days, the app SHALL find its
address on Workable from one job's short link (a redirect to the company's page, one request,
robots rules respected, at most a configured number a day), and SHALL upsert a Workable company
page in batches, recording "the Workable list", when the list last had it and how many jobs. The
app SHALL approve such a page by itself when the Workable hiring system is approved, without a
sample read, and the row's report SHALL say that the list itself shows the company posts jobs.
While the Workable system waits for the owner, its listed pages SHALL wait with it; a declined or
paused system SHALL NOT be read.

#### Scenario: Workable is approved
- **WHEN** the Workable hiring system is approved and the list has a company with a recent job
- **THEN** its company page is added and approved by the app, with no sample read, and its report says so

#### Scenario: Workable waits for the owner
- **WHEN** the Workable system waits because its robots.txt says `ai-train=no`
- **THEN** the listed company pages are added as candidates and none is approved until the owner approves the system

#### Scenario: A company found before
- **WHEN** a company page was already registered through an apply link and the list names it
- **THEN** no second page is added, and it gains when the list last had it and its job count

### Requirement: Searches read company pages through the store
A search SHALL treat the jobs of every approved company page in the job store as candidates, through the
cheap preselect, with no setting to turn that off.

#### Scenario: Thousands of company pages
- **WHEN** thousands of company pages are approved and read into the store
- **THEN** a search considers their jobs, and gives a quick look only to the closest ones

### Requirement: Each person has their own list of job sites, On or Off
The app SHALL keep, for each person, whether each approved job site is On or Off, and who set it
(the app or the person). Hiring systems and company pages SHALL NOT be on the list. A person's own
choice SHALL never be overwritten by the app, and turning a site back keeps it their choice. A site
that needs a key before it can be read SHALL be listed and SHALL NOT be turned On until the key is
set. A site that is not on the list, or that does not exist, SHALL be answered as not found.

#### Scenario: The person turns a site Off
- **WHEN** a person turns Remote OK Off
- **THEN** it is Off and stays Off when their goals change, and the list says nothing else about who set it

#### Scenario: A site that needs a key
- **WHEN** a person turns On a site that needs a key it does not have
- **THEN** the answer is a conflict `needs_key` and the site stays Off

#### Scenario: A company page
- **WHEN** a person asks to turn On a company page or a hiring system
- **THEN** the answer is not found, and it is not on their list

### Requirement: The app turns on the sites that fit the person's goals
For each approved site that needs no key and that the person has not chosen, the app SHALL set it On
or Off from the person's goals and the site's tags, and SHALL set it again whenever the list is
read, so changed goals and a site approved later are picked up. A site SHALL be On when: for a
remote-first site open worldwide or an all-jobs site, the person would take a remote job; for a
European site, they live in Europe or would move within the EU or anywhere, and the site has remote
jobs unless they would take office days; for a site in their country, the site has remote jobs
unless they would take office days; for a site built around a stack, a word of it is among their
skills or roles; for a narrow trade, their roles name that kind of work. A site whose kinds of jobs
do not match the kind of work their roles name SHALL be Off. A person with no answered goals SHALL
have every site Off, written nowhere. The app SHALL read the goals only through the port searches
use.

#### Scenario: A remote engineer in Lisbon
- **WHEN** a person in Lisbon who works remote only, as a backend engineer, has no choice on any site
- **THEN** the remote worldwide, the European sites with remote jobs, their country's with remote jobs and the all-jobs sites are On, and a Ruby site, a design site and a site of office jobs are Off

#### Scenario: The goals change
- **WHEN** the same person now wants office work
- **THEN** the sites the app had turned On are Off the next time the list is read, and the ones they chose stay as they are

#### Scenario: A site approved later
- **WHEN** a site is approved after the person's list was first read
- **THEN** the next reading sets it On or Off like the others

### Requirement: The list is grouped by what the sites offer
The list SHALL be in the groups Remote, worldwide; Europe; the person's country; By trade; Startups
and all-jobs sites, in that order, a group with no site left out. A site goes in the first group
that fits: By trade when it is built around a stack or lists one or two kinds of jobs none of them
tech; Startups and all-jobs when it lists startups; the person's country; Europe; Remote, worldwide
when it is remote-first and open worldwide; else Startups and all-jobs. Each site SHALL say what it
offers (its kinds of jobs, where, how remote, its speciality), its typical salary when known (the
average of the salaries its jobs state when at least ten state one, else its country's typical pay),
how many jobs are new on it in the last 7 days when the job store has a count, whether it is On, and whether it needs a key. The answer SHALL give how many sites are available and
how many are On, whatever is searched or shown. A site is found by part of its name, and the list
shows all sites, the ones that are On, remote ones, European ones, the person's country's or By trade.

#### Scenario: Find a site
- **WHEN** the person types "rub" in Find a site
- **THEN** only the sites with "rub" in their name are listed, and the counts stay those of the whole list

#### Scenario: Show the sites that are On
- **WHEN** the person shows the sites that are On
- **THEN** only those are listed, in their groups

### Requirement: Sites that do not allow apps are one sentence, not rows
The list SHALL not show a site whose terms or robots rules forbid an app reading it, or that needs
written permission first, as a row. It SHALL give their count and their names (the first ones), for
one sentence under the list, and SHALL give how many company hiring pages the app also reads.

#### Scenario: A site whose terms forbid apps
- **WHEN** a waiting site's terms forbid automated reading
- **THEN** it is not a row, and the sentence under the list names it and counts it

### Requirement: Company suggestions can be taken back
A person who said "Not interested" in a suggested company SHALL be able to take it back at once, and
the company SHALL be a suggestion again; for a company they never dropped the answer SHALL be not
found. Following a company again after stopping it is watching it again.

#### Scenario: Drop, then Undo
- **WHEN** a person drops a suggested company and then undoes it
- **THEN** the company is suggested again with its reasons

### Requirement: The Job sites page is one list for everyone, and the owners' view stays one tap away
The Job sites page SHALL show the person's list: a count line ("N available, M on"), a "Find a site"
box and one "Show" select, the groups with a name, one plain line, the typical salary and a single
On or Off button per site, "Add the key" for a site that needs one (the owner goes to the key, anyone
else is told the admin adds it), the sentence about sites that do not allow apps, a Companies section
and the count of company pages. Keep follows a company, and its row then reads "Following" with Stop;
every action shows Undo in the row and no row vanishes. The board owners SHALL also see "All sites",
the admin view, and it SHALL open first for them.

#### Scenario: Turn a site On
- **WHEN** a person presses Off on a site
- **THEN** the button reads On, the count of sites that are On goes up by one, and the choice is kept

#### Scenario: Keep a company
- **WHEN** a person presses Keep on a suggested company
- **THEN** its row reads "Following" with Stop and Undo is offered; the row does not leave the list

#### Scenario: An owner
- **WHEN** a board owner opens the Job sites page
- **THEN** "All sites" is open, with "Your sites" beside it

### Requirement: A posting is a title with a company, an apply link or its own job page
A sample item SHALL count as a posting when it has a title and a company, an apply link, or a link to
its own page on the site's host (or its feed's host) whose path names a job. A link to a blog post
SHALL NOT count, and an item linking to a blog post SHALL NOT count even with an author as its
company. A site whose sample jobs all apply on one hiring system's page for the same company SHALL
register that company page through the company pages' function, SHALL NOT be approved by the app
as a job site, and SHALL say so in its waiting sentence.

#### Scenario: A feed with no company and no apply link
- **WHEN** a feed's items have a title and a link to `/job/…` on the site
- **THEN** they count as postings

#### Scenario: A blog
- **WHEN** a feed's items link to `/blog/…` pages
- **THEN** none counts, and the site waits, saying how many items look like jobs

#### Scenario: One company
- **WHEN** every sample job applies at `jobs.lever.co/acme-labs/…`
- **THEN** the Acme Labs company page is registered, the site is not approved, and it says every job applies on one company's hiring page

### Requirement: The company is read from the titles when a pattern is consistent
For a feed with no company field the check SHALL read the company from the titles when at least 70 %
of the sample's titles follow one pattern ("Role at Company", "Company: Role", "Role - Company",
"Role | Company", "Company is hiring a Role", also with en and em dashes and `@`), with at least
three different companies, none of them in more than half of the matches. A pay in parentheses at the
end of the company SHALL be dropped. A pattern that is not that consistent, a company name over five
words, or the same name after every title SHALL NOT be applied. The pattern SHALL be kept in the
proposed config so later reads give the same company.

#### Scenario: Role at Company
- **WHEN** a feed's titles are "Rust Engineer at Example Labs ($45k-$60k)" and five more like it
- **THEN** the company is "Example Labs" and the title "Rust Engineer"

#### Scenario: The site's name after every title
- **WHEN** every title ends " - Board Jobs"
- **THEN** no company is read

### Requirement: A site is alive by its dates
A site SHALL be alive when a sample job was posted in the last 30 days. When no sample job has a date,
the feed's own date (`lastBuildDate`, or `updated`) or the `Last-Modified` the site answered with
SHALL count when it is within 30 days. A feed whose jobs are dated and old SHALL NOT be alive by its
own date.

#### Scenario: A feed with no item dates
- **WHEN** a feed's items have no date and the site answered with a `Last-Modified` two days old
- **THEN** the site is alive

#### Scenario: Old and undated
- **WHEN** the items have no date and the newest change is three months old
- **THEN** the site waits, saying none of its jobs says when it was posted

### Requirement: The check looks for a site's list of jobs in more places, within a request budget
When a board has no usable config, the check SHALL look for its list of jobs in this order, asking each
address once and only where the site's robots rules allow: the feeds the homepage links to; the public
address of a job-board platform recognised from its markup (WP Job Manager at `/feed/job_feed/`, Job
Boardly at `/jobs.xml`, JBoard at `/rss/jobs`, Niceboard through its sitemap); the site's own Jobs
page linked from the homepage and the feeds it links to; the sitemaps robots.txt lists, else
`/sitemap.xml`, followed through sitemap indexes down to the sitemap of jobs, with five of its pages
read for `JobPosting` JSON-LD as the sample; then `/jobs.rss`, `/jobs/feed`, `/feed/jobs`,
`/jobs.xml`, `/feed`, `/rss`, `/feed.xml`, `/rss.xml`, `/jobs.json`, `/api`, `/jobs/api`,
`/?feed=job_feed` and `/feed/?post_type=job_listing`. One check SHALL make at most 25 requests after
robots.txt and the homepage (`APPLICAMP_SOURCES__DISCOVERY_MAX_REQUESTS`, 5 to 100), and when it
stops for that reason its waiting sentence SHALL say so.

#### Scenario: A sitemap index
- **WHEN** robots.txt lists a sitemap index naming `sitemap-jobs.xml`, `sitemap-companies.xml` and `sitemap-blog.xml`
- **THEN** only the sitemap of jobs is read, five of its job pages are read for JSON-LD, and the config points at that sitemap with the jobs' path

#### Scenario: A platform whose feed the rules disallow
- **WHEN** a JBoard site's robots.txt disallows `/rss/`
- **THEN** its feed is never asked for and its sitemap is read instead

#### Scenario: The budget
- **WHEN** nothing is found and the budget is 7
- **THEN** the site got robots.txt, its homepage and 7 more requests, and the report says the check used its 7 requests

### Requirement: The terms page is also looked for at its usual paths
When the homepage links to no terms page, the check SHALL use a terms link on the site's Jobs page,
and else try `/terms`, `/terms-of-service`, `/tos`, `/legal` and `/terms-and-conditions` in turn,
stopping at the first that is a terms page (a heading that says so and a page of text, never a page
that answers every address alike). The terms SHALL be read and the verdict decided exactly as before.

#### Scenario: A usual path
- **WHEN** the homepage has no terms link and `/terms-of-service` is a page titled "Terms of Service"
- **THEN** its sentences are kept and `/tos` is not asked for

#### Scenario: A site that answers every path with its front page
- **WHEN** every path returns the same page with no terms heading
- **THEN** no terms page is found and the report says so

### Requirement: Candidates are checked again once under new rules
Candidates that wait SHALL be checked before new finds: first those never checked (suggested or
seeded), then those checked under older rules, oldest check first. Together these two passes SHALL
take at most half of the day's checks (`APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS`, 100, so 50), and
the new finds from the public lists SHALL take the rest of the day's checks, at least that other
half and whatever the first two passes leave unused. All of it stays within the AI caps. A candidate
checked under the current rules SHALL be left alone. Terms SHALL be read by the model again only
when their text changed. Each run SHALL count the candidates it checked again under new rules on
their own (`rechecked_candidates`), apart from the approved boards due a re-check (`rechecked`).

#### Scenario: More candidates than the day's checks
- **WHEN** three candidates were checked under old rules and the day allows four checks
- **THEN** the two with the oldest checks are checked, the third the next day, and none after that

#### Scenario: Old candidates and many new finds
- **WHEN** 104 candidates were checked under old rules and the lists offer 100 new sites
- **THEN** 50 old ones are re-checked and 50 new ones are checked, and the next day the next 50 old ones

#### Scenario: Never checked comes first
- **WHEN** a suggested candidate was never checked and others were checked under older rules
- **THEN** the suggested candidate is checked before any of the older ones

#### Scenario: No old work
- **WHEN** no candidate waits to be checked or checked again
- **THEN** new finds from the lists get all of the day's checks
