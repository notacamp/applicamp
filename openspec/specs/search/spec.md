# search Specification

## Purpose
Finding jobs worth a person's time: public job-board APIs read and filtered by the goals' hard
limits, the rest scored against the profile and goals by the model, the good matches added to
the pipeline; Today saying what to do next; and every model call kept within spending limits
in money that the person can see.

## Requirements

### Requirement: Search public job boards against your goals
The app SHALL read jobs only from public job-board APIs (Greenhouse, Lever,
Ashby, Arbeitnow, HN "Who is hiring"), drop jobs that break the goals' hard
limits, score the rest against the profile and goals, and add the good
matches to the pipeline as Found with the reasons and gaps. It SHALL never
read LinkedIn or Indeed. Hard limits SHALL be decided by code from facts
read from each posting, never by the score: a job that is not confirmed
remote when they work remote only, that its posting limits to other places
than where they live when they work remote only or would not move, that
needs a language they did not list, or whose stated salary is below their
floor SHALL NOT be added. An unclear fact SHALL count as not met, except
whether a remote job hires where they live: a remote job whose posting does
not say SHALL be kept with the gap "Doesn't say if it hires in" their
country. A match reason SHALL cite only what the profile and goals say:
every technology, framework, language, company, job title, place and number
a reason attributes to the person SHALL appear in their profile or goals, a
reason that does not SHALL be dropped, and a job whose every reason was
dropped SHALL NOT be added. The model SHALL be told what the profile does
not contain and SHALL NOT infer a technology stack. Gaps MAY name the
posting's requirements.

#### Scenario: Search now
- **WHEN** someone with a profile and goals presses Search now
- **THEN** they see progress, and the matches appear as Found with a score, reasons and gaps

#### Scenario: A job that tries to give instructions
- **WHEN** a job description contains text telling the model to give it a perfect score
- **THEN** the job is scored like any other and nothing else changes

#### Scenario: Remote only from another country
- **WHEN** someone in Lisbon who works remote only searches and a posting says "remote-first from Germany or Austria"
- **THEN** that job is not added, whatever its score

#### Scenario: Where it hires not stated
- **WHEN** someone in Lisbon who works remote only searches and a remote posting does not say where it hires
- **THEN** the job is kept, and when it is a good match it is added with the gap "Doesn't say if it hires in Portugal", shown in the person's language

#### Scenario: Remote not stated
- **WHEN** a posting does not say whether it is remote and the person works remote only
- **THEN** that job is not added

#### Scenario: A language they don't work in
- **WHEN** a posting requires German and the person did not list German
- **THEN** that job is not added

#### Scenario: An invented stack
- **WHEN** a profile lists no skills and the model answers a match with the reason "Full-stack TypeScript matches profile"
- **THEN** that reason is dropped, and if it was the only one the job is not added and is counted as not grounded

#### Scenario: A reason the profile supports
- **WHEN** a profile lists Ruby and Rails and the model answers "Rails experience matches"
- **THEN** the reason is kept on the match

#### Scenario: One reason holds, one does not
- **WHEN** the model gives "Rails experience matches" and "Deep TypeScript expertise" for a profile with Ruby and Rails
- **THEN** only the first is kept and the job is added on it

#### Scenario: The prompt says what is missing
- **WHEN** a profile lists no skills
- **THEN** the model is told "No skills are listed. Do not infer a technology stack."

#### Scenario: Old scores are asked again
- **WHEN** the reasons rules change version and a job was scored under the old one
- **THEN** the next search scores it again, once

### Requirement: Today shows where you are
Today SHALL show the person's next step from their state: set up the profile,
answer the goals, run a first search, or review new matches.

#### Scenario: Profile and goals done
- **WHEN** someone with a profile and answered goals opens Today before any search
- **THEN** Today offers Search now instead of asking them to set up their profile

### Requirement: Spending stays within limits
Every model call SHALL be metered in money and refused before it would cross
the account's daily or monthly limit or the instance's monthly limit; the
person SHALL see what they spent and an estimate before a search. Unless the
instance sets others, an account's limits SHALL be €1 a day and €10 a month,
and the instance's €15 a month.

#### Scenario: The monthly limit is reached
- **WHEN** an account's monthly AI spend has reached its limit and a search runs
- **THEN** no model call is made, the search ends saying the limit was reached, and the jobs already scored stay in the pipeline

#### Scenario: The same job twice
- **WHEN** a job was already scored for the current profile and goals
- **THEN** the next search reuses that score and makes no model call for it

#### Scenario: The default limits
- **WHEN** the instance sets no spending limits
- **THEN** each account may spend €1 a day and €10 a month, and the instance €15 a month

### Requirement: Existing matches are checked again
When the requirement rules change, opportunities still Found or Shortlisted
SHALL be checked again: the ones that fail SHALL be hidden with the reason,
never deleted, and SHALL stay viewable, and the ones hidden only because
their posting did not say whether it hires where the person lives SHALL be
shown again with that gap, without a model call.

#### Scenario: A match from before the fix
- **WHEN** the rules are deployed and a Found job turns out to be remote only within Germany
- **THEN** it moves out of view into "Hidden: don't meet your requirements" with that reason

#### Scenario: Hidden for not saying where it hires
- **WHEN** the server starts and a Found remote job was hidden because its posting did not say whether it hires in Portugal
- **THEN** it is back in view with the gap "Doesn't say if it hires in Portugal", unless it fails another requirement

### Requirement: Search reads approved job boards and credits them
A search SHALL use only job boards the owner approved, honour each board's
terms (polling limits, attribution, unchanged apply links, expiry), merge the
same job seen on several boards into one opportunity, and show every source
with a link to the original posting.

#### Scenario: The same job on two boards
- **WHEN** a job with the same apply link is in the store from Himalayas and RemoteOK
- **THEN** one opportunity is added, showing both boards as sources with links

#### Scenario: A board's polling limit
- **WHEN** a search runs twice within Remotive's allowed interval
- **THEN** Remotive is not requested by either search, and the store's jobs are used

### Requirement: Existing reasons are checked against the profile
Opportunities still Found or Shortlisted that were found before reasons were
grounded SHALL have their reasons checked once, without a model call: the
reasons the profile and goals do not support SHALL be removed and the
opportunity SHALL show "Reasons updated". Nothing SHALL be hidden, deleted or
moved to another stage for this.

#### Scenario: A match from before the check
- **WHEN** the check is deployed and a Found match has the reason "Full-stack TypeScript matches profile" for a profile with no skills
- **THEN** the reason is removed, its peek says "Reasons updated", and the opportunity stays where it was

### Requirement: Today and the Pipeline say when the profile has no stack
When the profile lists no skills and names no technology, the Pipeline's
search line and Today SHALL say "Your profile lists no skills yet. Add them
for better matches", linking to Skills on Profile & goals.

#### Scenario: No skills
- **WHEN** someone whose profile lists no skills opens Today or the Pipeline
- **THEN** they see that line, and its link opens Skills on Profile & goals

### Requirement: Search reads approved boards by your roles and top skills
For a person whose goals name roles or whose profile lists skills, a search
SHALL read each approved board that can search by keyword with one extra
query for each of the first 3 wanted roles, without seniority words ("Senior
Ruby Developer" is "ruby developer"), and for each of the first 3 skills in
the person's order, without repeats and at most 6, in addition to the
board's plain read, one page each; merge and deduplicate the answers, and
rank jobs that name a top skill higher. At night a board SHALL be asked
everyone's queries in turn, at most 6 in all, the same for the nightly
reading and every nightly search. It SHALL stay within each board's polling
limits and terms and add no model call. With no roles and no skills, or on
a board with no keyword search, it SHALL read as before.

#### Scenario: A Ruby on Rails developer
- **WHEN** the goals want "Senior Ruby Developer" and the profile's first skills are Ruby on Rails, React and PostgreSQL, and a search runs
- **THEN** Himalayas is asked for "ruby developer", "ruby on rails", "react" and "postgresql" as well as read plainly, Remote OK for the tags `ruby-developer`, `ruby`, `react` and `postgres`, the jobs are merged without duplicates, and the ones naming those skills rank first

#### Scenario: A board with no skill filter
- **WHEN** a search runs and Arbeitnow is approved
- **THEN** Arbeitnow is read once, plainly

#### Scenario: A board's fetch limit
- **WHEN** a board allows 24 fetches a day and six queries are added
- **THEN** each of its seven addresses is reused for 7 hours, so it is fetched at most 24 times a day

#### Scenario: Several people at night
- **WHEN** two people with goals each have four queries and the nightly search runs
- **THEN** each board that searches is asked six of them, their first ones in turn, and no more that night

#### Scenario: No skills
- **WHEN** the goals name no role and the profile lists no skills
- **THEN** every board is read exactly as before

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

### Requirement: One bad model answer never ends a search
A search SHALL score a batch, of quick looks or of full reads, whose model
answer is unusable (cut off, or not the JSON asked for) again as two halves,
and SHALL leave the jobs of a half that fails again for the next search.
Only four failed calls in a row SHALL end a search's scoring (`ai_failed`),
keeping what was scored. On OpenRouter, scoring calls SHALL ask for low
reasoning effort and SHALL be routed only to providers that support every
parameter sent.

#### Scenario: An answer is cut off
- **WHEN** the answer for a batch of ten is cut off and the answers for five jobs are not
- **THEN** the ten are scored as two batches of five, the search goes on, and it ends without `ai_failed`

#### Scenario: The provider keeps failing
- **WHEN** every scoring answer is unusable
- **THEN** the search makes four calls, then ends its scoring with `ai_failed`

### Requirement: A search also reads the watched companies' job boards once a day
A search SHALL read the own job board of each company the person watches when it was last read more than 20 hours ago (the setting
`watch_every_secs`), compare it with the postings already seen, and send only the new ones through the search's own hard limits, scoring,
requirement rules and spending limits, in the same batches. A match SHALL be added to the pipeline as Found, credited to the company's own
job board and deduplicated against what the pipeline holds. What the search did on each watched board SHALL be kept as a scan row. When a
board has nothing new, no model SHALL be called for it. The nightly search SHALL start for a person whose watched board is due even when no
job board has anything new, and the nightly job SHALL still schedule the next night's while it runs.

#### Scenario: One new job among three
- **WHEN** a watched board lists three jobs, two already seen
- **THEN** only the new one is scored, and if it fits it is added as Found credited to the company's job board

#### Scenario: Nothing new
- **WHEN** a watched board lists only postings already seen
- **THEN** no model is called for it and its scan row says it read the board and matched nothing

#### Scenario: A hard limit before scoring
- **WHEN** a new posting is on site and the person works remote only
- **THEN** it is never sent to the model

#### Scenario: A requirement it fails
- **WHEN** a new posting is open only to Germany and Austria and the person lives in Portugal
- **THEN** it is scored, dropped for the requirements, and not added

#### Scenario: A failed board
- **WHEN** a watched board answers 503
- **THEN** the watch is kept with "last check failed", the scan row says why, and the board is not asked again until the next day

#### Scenario: Not scored for lack of budget
- **WHEN** the model cannot score a new posting
- **THEN** it is not noted as seen, and the next search scores it

#### Scenario: A due watch starts the nightly search
- **WHEN** the nightly job runs for a person whose watched board is due and no job board has anything new
- **THEN** it starts their search, and the next night's nightly job is scheduled

### Requirement: Every job within your limits gets a quick look, and the best are read in full
A search SHALL give every job within the hard limits that has no score for
the current profile, goals and rules a quick look: one model call per 25
jobs with each job's title, company, place, way of working and the first 400
characters of its posting, answering a fit from 0 to 100 and one short
reason, kept for later searches. It SHALL then read in full, as before, the
jobs with the best quick fit, at most 100 a search (the setting
`FULL_SCORES_PER_SEARCH`), and add the good matches. A job read in full SHALL
never take a slot again while the profile, goals and rules are the same.
When every job without a full read fits in the slots, the search SHALL read
them all in full with no quick look. A job whose description is not read yet
SHALL be judged from its title, company and place, and the model SHALL be
told so. An answer that names no job of its batch SHALL count as a failed
call. A watched company's new posting SHALL be
read in full ahead of the cut. Every call SHALL stay within the spending
limits, and the search SHALL stop looking and reading after ten minutes,
keeping what it has. The nightly search SHALL start for a person when a job
within their limits has neither a quick look nor a full read.

#### Scenario: A night with many jobs
- **WHEN** 958 jobs pass a person's hard limits and none was looked at before
- **THEN** each gets a quick look, 25 a call, and the best 100 by fit are read in full

#### Scenario: Few jobs
- **WHEN** 30 jobs pass the hard limits and none was read in full
- **THEN** all 30 are read in full, with no quick look

#### Scenario: Read before
- **WHEN** a job was read in full by an earlier search for the same profile, goals and rules
- **THEN** it is not read again and takes no slot

#### Scenario: Looked at before
- **WHEN** a job had a quick look last night and was not among the best
- **THEN** it gets no new quick look, and tonight its fit competes again for a full read

#### Scenario: A limit during the quick looks
- **WHEN** the account's daily limit is reached during the quick looks
- **THEN** no further call is sent, what was looked at is kept, and the search ends saying the limit was reached

#### Scenario: An answer about no job of the batch
- **WHEN** the model answers every quick look with references that are not the batch's
- **THEN** each answer counts as a failed call, it is asked again in halves once, and four in a row end the search with `ai_failed`

#### Scenario: Nothing new for the nightly search
- **WHEN** every job within a person's limits already has a quick look or a full read
- **THEN** no nightly search starts for them

### Requirement: A page that fails keeps the pages already read
A search SHALL keep the jobs of the pages of a site it already read when a
later page fails, and the site's line SHALL say which page did not answer. A
first page that fails SHALL fail the site for that search, as before. A site
that asks to slow down part way SHALL be asked nothing more then, and SHALL
be left alone for at least its polling interval, or as long as it asked.

#### Scenario: The third page answers 500
- **WHEN** a site's first two pages answer and the third answers 500
- **THEN** the jobs of the first two are used, and the site's line says page 3 did not answer

#### Scenario: Asked to slow down on page 4
- **WHEN** Himalayas answers its fourth page with 429 and `Retry-After: 600`
- **THEN** its first three pages are used, its queries are not sent, and a search 15 minutes later does not ask it

### Requirement: A search draws its jobs from the job store
A search SHALL take its candidates from the job store: the open jobs of the person's On job sites and
of every approved company page (the companies the person watches included), and SHALL NOT read those
sites or pages live. It SHALL read live only the person's own queries (their wanted roles and top
skills) on the sites that search by keyword, because the store reads only default feeds, and merge and
deduplicate those jobs with the store's. A nightly search SHALL take only the jobs the store first saw
since that person's last search. With the store turned off a search SHALL read the sites live.

#### Scenario: A search does not read the sites
- **WHEN** a site was read into the store and a person searches
- **THEN** the search finds the site's jobs without a request to the site

#### Scenario: Company pages
- **WHEN** thousands of company pages are approved and a person searches
- **THEN** their jobs are candidates, through the cheap preselect

#### Scenario: Only new jobs at night
- **WHEN** a nightly search runs and the person's last search was this afternoon
- **THEN** it takes only the jobs the store first saw since that search

#### Scenario: A night with new jobs always runs
- **WHEN** the store first saw jobs since a person's last search, they pass the person's hard limits and have no quick look or full read
- **THEN** the nightly job starts their search

### Requirement: A cheap preselect limits the quick looks
Before any model call, a search SHALL rank the jobs without a look from the store with Postgres full
text (the title, company and tags against the words of the person's wanted roles and top skills) plus
how new they are, and SHALL give a quick look only to the top `QUICK_LOOK_PER_SEARCH` (400). The others
SHALL be counted as "Not among the closest to your roles today", so the reasons still add up to the jobs
read, and be considered by a later search. It SHALL record the cost it expects before its first call, and
the estimate SHALL count the store's jobs without a score, at most that limit.

#### Scenario: A store of 40,000 jobs
- **WHEN** a nightly search finds 1,500 new jobs without a look
- **THEN** 400 get a quick look (16 calls), the best 100 are read in full (10 calls), and 1,100 are counted as not among the closest

#### Scenario: Looked at before
- **WHEN** a job had a quick look in an earlier search
- **THEN** it takes no slot of the 400 and competes again for a full read
