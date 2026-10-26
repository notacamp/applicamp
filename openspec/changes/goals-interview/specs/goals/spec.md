## ADDED Requirements

### Requirement: An interview that never forces
The app SHALL ask about goals one question per screen; every question SHALL
be skippable, the person SHALL be able to leave at any time with what they
answered saved, and nothing SHALL block the rest of the app until it is done.

#### Scenario: Skip and leave
- **WHEN** someone answers the roles question, skips the salary question and chooses Finish later
- **THEN** their roles are saved, the salary stays unanswered, and they land on Today

### Requirement: Versioned, editable goals
Every change to the goals SHALL create a new version, and each answer SHALL
be editable from Profile & goals.

#### Scenario: Editing the cash floor
- **WHEN** someone changes their cash floor from €90,000 to €95,000 a year
- **THEN** the goals version goes up by one and the new floor shows on Profile & goals
