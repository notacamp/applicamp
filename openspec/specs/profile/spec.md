# profile Specification

## Purpose
The master profile: what Applicamp knows about a person's work (headline, summary,
positions, education, skills, languages, certifications, projects, links), imported from
a LinkedIn data export, a PDF or pasted text as a draft they review, and edited by hand.
Agents use only what it says, and nothing in it is invented.

## Requirements

### Requirement: Import a profile for review
The app SHALL turn a LinkedIn data export ZIP, a PDF (LinkedIn profile or CV)
or pasted text into a draft profile that the person reviews, and SHALL save
nothing to the profile until they choose Replace or Merge. Uploaded files
SHALL NOT be stored. A model-read draft SHALL ask for every section (headline,
summary, positions, education, skills, languages, certifications, projects),
SHALL never be saved from an answer the model cut off or that is unusable, and
the review SHALL list the sections that came back empty.

#### Scenario: A LinkedIn export
- **WHEN** a signed-in person uploads their LinkedIn export ZIP
- **THEN** a draft holds their positions, education, skills and languages from its CSVs, without any model call, and their profile is unchanged until they apply it

#### Scenario: A CV as PDF
- **WHEN** they upload a CV PDF with text
- **THEN** a draft holds the structured CV and says how many details were left out because they did not appear in the file

#### Scenario: A LinkedIn profile as PDF
- **WHEN** they upload their LinkedIn profile saved as a two-column PDF with a headline, an About text, skills, education and languages
- **THEN** the draft holds the headline, the summary, the skills, the education and the languages as well as the positions

#### Scenario: An answer cut off
- **WHEN** the model stops for length or answers something that is not the expected JSON, even after one more try
- **THEN** the import fails with a message saying so and nothing is created

#### Scenario: Parts that came back empty
- **WHEN** a draft has no skills
- **THEN** the review says "No skills found in this file" and suggests adding them by hand or importing the LinkedIn data export

#### Scenario: A scanned PDF
- **WHEN** the PDF has no extractable text
- **THEN** the import fails with a message saying so and nothing is created

#### Scenario: A hostile archive
- **WHEN** a ZIP holds a path outside the archive, too many entries or more data than the limits once uncompressed
- **THEN** the import is refused before anything is extracted

### Requirement: Nothing invented
A draft structured by a model SHALL contain only values that appear in the
source text; any other value SHALL be dropped and counted.

#### Scenario: A made-up employer
- **WHEN** the model returns a company name that is not in the CV text
- **THEN** that position is not in the draft and the dropped count includes it

### Requirement: Edit the profile
The person SHALL be able to see and edit every part of their profile, and an
import SHALL be able to merge into it without losing what they edited.

#### Scenario: Merge keeps edits
- **WHEN** they merge a new import into a profile they edited by hand
- **THEN** their edited values stay and only new positions, schools and skills are added

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
