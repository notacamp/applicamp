## RENAMED Requirements
- FROM: `### Requirement: Search reads approved boards by your top skills`
- TO: `### Requirement: Search reads approved boards by your roles and top skills`

## MODIFIED Requirements

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

### Requirement: Search reads approved boards by your roles and top skills
For a person whose goals name roles or whose profile lists skills, a search
SHALL read each approved board that can search by keyword with one extra
query for each of the first 3 wanted roles, without seniority words ("Senior
Ruby Developer" is "ruby developer"), and for each of the first 3 skills in
the person's order, without repeats and at most 6, in addition to the
board's plain read, one page each; merge and deduplicate the answers, and
rank jobs that name a top skill higher. It SHALL stay within each board's
polling limits and terms and add no model call. With no roles and no
skills, or on a board with no keyword search, it SHALL read as before.

#### Scenario: A Ruby on Rails developer
- **WHEN** the goals want "Senior Ruby Developer" and the profile's first skills are Ruby on Rails, React and PostgreSQL, and a search runs
- **THEN** Himalayas is asked for "ruby developer", "ruby on rails", "react" and "postgresql" as well as read plainly, Remote OK for the tags `ruby-developer`, `ruby`, `react` and `postgres`, the jobs are merged without duplicates, and the ones naming those skills rank first

#### Scenario: A board with no skill filter
- **WHEN** a search runs and Arbeitnow is approved
- **THEN** Arbeitnow is read once, plainly

#### Scenario: A board's fetch limit
- **WHEN** a board allows 24 fetches a day and six queries are added
- **THEN** each of its seven addresses is reused for 7 hours, so it is fetched at most 24 times a day

#### Scenario: No skills
- **WHEN** the goals name no role and the profile lists no skills
- **THEN** every board is read exactly as before

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

## ADDED Requirements

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
them all in full with no quick look. A watched company's new posting SHALL be
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

#### Scenario: Nothing new for the nightly search
- **WHEN** every job within a person's limits already has a quick look or a full read
- **THEN** no nightly search starts for them

### Requirement: A page that fails keeps the pages already read
A search SHALL keep the jobs of the pages of a site it already read when a
later page fails, and the site's line SHALL say which page did not answer. A
first page that fails SHALL fail the site for that search, as before.

#### Scenario: The third page answers 500
- **WHEN** a site's first two pages answer and the third answers 500
- **THEN** the jobs of the first two are used, and the site's line says page 3 did not answer
