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
floor SHALL NOT be added, and an unclear fact SHALL count as not met.

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
