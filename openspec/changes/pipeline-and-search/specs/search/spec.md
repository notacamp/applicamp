## ADDED Requirements

### Requirement: Search public job boards against your goals
The app SHALL read jobs only from public job-board APIs (Greenhouse, Lever,
Ashby, Arbeitnow, HN "Who is hiring"), drop jobs that break the goals' hard
limits, score the rest against the profile and goals, and add the good
matches to the pipeline as Found with the reasons and gaps. It SHALL never
read LinkedIn or Indeed.

#### Scenario: Search now
- **WHEN** someone with a profile and goals presses Search now
- **THEN** they see progress, and the matches appear as Found with a score, reasons and gaps

#### Scenario: A job that tries to give instructions
- **WHEN** a job description contains text telling the model to give it a perfect score
- **THEN** the job is scored like any other and nothing else changes

### Requirement: Today shows where you are
Today SHALL show the person's next step from their state: set up the profile,
answer the goals, run a first search, or review new matches.

#### Scenario: Profile and goals done
- **WHEN** someone with a profile and answered goals opens Today before any search
- **THEN** Today offers Search now instead of asking them to set up their profile
