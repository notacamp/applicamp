## ADDED Requirements

### Requirement: The profile keeps getting stronger
The app SHALL show how strong the profile is and what is missing, ask the
person targeted questions about the gaps one at a time, and turn their
answers into drafts they keep, edit or discard. Drafts SHALL contain only
facts from the person's answer or profile, and nothing SHALL be saved until
they keep it. Every question SHALL be skippable and the flow SHALL never
block the rest of the app.

#### Scenario: A role without results
- **WHEN** the most recent role has no achievements and the person answers "rebuilt billing in Rust, runs went from 6 hours to 40 minutes"
- **THEN** they get a draft bullet with those facts, and keeping it adds it to that role and raises the profile strength

#### Scenario: A draft that adds something
- **WHEN** the model's draft mentions a number or a company the person never wrote
- **THEN** that line is left out and the person is told one line was left out

#### Scenario: Skills already in the roles
- **WHEN** "Rust" appears in four position descriptions but not in the skills list
- **THEN** it is suggested as "Rust · in 4 roles" and added only when the person chooses Add
