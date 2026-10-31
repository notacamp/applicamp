## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Existing matches are checked again
When the requirement rules change, opportunities still Found or Shortlisted
SHALL be checked again and the ones that fail SHALL be hidden with the reason,
never deleted, and SHALL stay viewable.

#### Scenario: A match from before the fix
- **WHEN** the rules are deployed and a Found job turns out to be remote only within Germany
- **THEN** it moves out of view into "Hidden: don't meet your requirements" with that reason
