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
remote and open to the person's location when they work remote only, that
needs a language they did not list, or whose stated salary is below their
floor SHALL NOT be added, and an unclear fact SHALL count as not met. A
match reason SHALL cite only what the profile and goals say: every
technology, framework, language, company, job title, place and number a
reason attributes to the person SHALL appear in their profile or goals, a
reason that does not SHALL be dropped, and a job whose every reason was
dropped SHALL NOT be added. The model SHALL be told what the profile does not
contain and SHALL NOT infer a technology stack. Gaps MAY name the posting's
requirements.

#### Scenario: Search now
- **WHEN** someone with a profile and goals presses Search now
- **THEN** they see progress, and the matches appear as Found with a score, reasons and gaps

#### Scenario: A job that tries to give instructions
- **WHEN** a job description contains text telling the model to give it a perfect score
- **THEN** the job is scored like any other and nothing else changes

#### Scenario: Remote only from another country
- **WHEN** someone in Lisbon who works remote only searches and a posting says "remote-first from Germany or Austria"
- **THEN** that job is not added, whatever its score

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
person SHALL see what they spent and an estimate before a search.

#### Scenario: The monthly limit is reached
- **WHEN** an account's monthly AI spend has reached its limit and a search runs
- **THEN** no model call is made, the search ends saying the limit was reached, and the jobs already scored stay in the pipeline

#### Scenario: The same job twice
- **WHEN** a job was already scored for the current profile and goals
- **THEN** the next search reuses that score and makes no model call for it

### Requirement: Existing matches are checked again
When the requirement rules change, opportunities still Found or Shortlisted
SHALL be checked again and the ones that fail SHALL be hidden with the reason,
never deleted, and SHALL stay viewable.

#### Scenario: A match from before the fix
- **WHEN** the rules are deployed and a Found job turns out to be remote only within Germany
- **THEN** it moves out of view into "Hidden: don't meet your requirements" with that reason

### Requirement: Search reads approved job boards and credits them
A search SHALL read only job boards the owner approved, honour each board's
terms (polling limits, attribution, unchanged apply links, expiry), merge the
same job seen on several boards into one opportunity, and show every source
with a link to the original posting.

#### Scenario: The same job on two boards
- **WHEN** a job with the same apply link is found on Himalayas and RemoteOK
- **THEN** one opportunity is added, showing both boards as sources with links

#### Scenario: A board's polling limit
- **WHEN** a search runs twice within Remotive's allowed interval
- **THEN** Remotive is not fetched the second time and its cached jobs are used

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

### Requirement: Search reads approved boards by your top skills
For a person whose profile lists skills, a search SHALL read each approved board that can filter by
skill with one extra query per top skill (the first 3 skills, in the person's order), in addition
to the board's plain read, merge and deduplicate the answers, and rank jobs that name a top skill
higher before scoring. It SHALL stay within each board's polling limits and terms, add no model
call, and leave the scorer's caps unchanged. With no skills, or on a board with no skill filter,
it SHALL read as before.

#### Scenario: A Ruby on Rails developer
- **WHEN** the profile's first skills are Ruby on Rails, React and PostgreSQL and a search runs
- **THEN** RemoteOK is read for the tags `ruby`, `react` and `postgresql` as well as plainly, Jobicy for the keywords `ruby-on-rails`, `react` and `postgresql`, the jobs are merged without duplicates, and the ones naming those skills rank first

#### Scenario: A board with no skill filter
- **WHEN** a search runs and Arbeitnow is approved
- **THEN** Arbeitnow is read once, plainly

#### Scenario: A board's fetch limit
- **WHEN** a board allows 24 fetches a day and three stack queries are added
- **THEN** each of its four addresses is reused for 4 hours, so it is fetched at most 24 times a day

#### Scenario: No skills
- **WHEN** the profile lists no skills
- **THEN** every board is read exactly as before
