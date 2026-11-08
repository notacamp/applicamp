## MODIFIED Requirements

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
