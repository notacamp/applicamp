## ADDED Requirements

### Requirement: Longer writing has its own model
An instance MAY configure a `write` model (`APPLICAMP_AI__WRITE__…`, the same settings as
`complete`) for longer writing; the tailored CV SHALL be drafted on it when it is set, and on the
`complete` model when it is not. Its calls SHALL be metered in the same ledger with their role, under
the same daily cap and spending limits, and refused before they would cross a limit.

#### Scenario: One model
- **WHEN** an instance sets only the `complete` model and someone drafts a CV
- **THEN** the CV is drafted on the `complete` model

#### Scenario: Two models
- **WHEN** an instance sets a `write` model too
- **THEN** the CV is drafted on it, and the ledger records the call under `write`
