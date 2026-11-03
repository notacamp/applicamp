## ADDED Requirements

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
