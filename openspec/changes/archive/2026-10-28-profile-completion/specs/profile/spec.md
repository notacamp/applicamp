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

#### Scenario: Skills in order
- **WHEN** the person adds a skill found in their roles and moves it up on the Skills page
- **THEN** it is listed where they put it, the first three are marked as the ones the job board searches use, and nothing is added to or removed from the list by the move

#### Scenario: Not mine
- **WHEN** the person marks a suggested skill "Not mine"
- **THEN** it is not suggested again, whatever the review says later

#### Scenario: No model, or over a spending limit
- **WHEN** no AI model is configured, or the spending limit is reached
- **THEN** the checklist still shows what to add, the Improve page says why there are no questions (and, over a limit, that nothing was sent), and nothing is retried until the person asks again

#### Scenario: One review per version
- **WHEN** the person answers and keeps several drafts, adds skills and reorders them
- **THEN** the AI reads the profile once for that version: the questions and skills carry on, and only a change made another way (an import, an edit by hand, new goals) asks the model again

#### Scenario: Not now
- **WHEN** the person chooses "Not now" on Today's profile strength card
- **THEN** it is hidden for a week, and every other part of the app works as before

### Requirement: A draft is checked against the person's own words
A draft's numbers, percentages, amounts, names and technologies SHALL each
appear in the answer or the profile, else the line SHALL be left out and
counted, and a draft SHALL be kept only for the profile version it was made
from.

#### Scenario: The profile changed since the draft
- **WHEN** the person edits their profile after a draft was made and then keeps it
- **THEN** keeping is refused with `profile_changed` and nothing is written
