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

## ADDED Requirements

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
