## MODIFIED Requirements

### Requirement: A CV can add a fact or a skill in the person's own words
When the person says a line of a tailored CV is true, the profile SHALL keep
their words as a fact on the position the line is about, with those words as its
quote and an area (`results` by default), as a new profile version, as keeping a
proposed fact does. When they answer the app's question about a line with "Yes,
keep it", the profile SHALL keep the fact the app proposed as a **confirmed
assumption** on that position, in area `results`, with what it was based on ("The
CV draft for <company>"), shown as such on the profile and used as evidence like
any other fact. The position SHALL still be the one the CV was made from, else it is
refused (`profile_changed`). Adding a skill SHALL add it at the end of the skills
unless the profile already lists it under another spelling, and SHALL forget an
earlier "not mine" for it.

#### Scenario: A fact from a CV
- **WHEN** someone says a blocked line is true and writes it in their own words
- **THEN** the words are a kept fact on that position and appear under it on the profile

#### Scenario: Yes to the app's question
- **WHEN** someone answers "Yes, keep it" to "Is this true?" about a line of their CV
- **THEN** the proposed fact is on that position, marked as a confirmed assumption based on the CV draft for that company

#### Scenario: The position changed
- **WHEN** the position was renamed or removed since the draft was made
- **THEN** nothing is added and the person is told to make the CV again
