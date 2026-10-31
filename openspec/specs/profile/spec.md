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

### Requirement: Grill me, a deep interview about each role
The app SHALL offer an interview about one role at a time that asks one
question at a time, in the person's language, through seven areas in order
(context, their part, stack and scale, results, hard problems, people,
stories). It SHALL push back on vague answers, ask for numbers and how the
person knows them, ask what they did themselves as against the team, and ask
about contradictions between the new answer, earlier facts and the profile.
Every question SHALL be skippable, the person SHALL be able to leave a role
for now and continue later, and nothing SHALL be saved to the profile until
they keep it.

#### Scenario: A vague answer
- **WHEN** the person answers a question with "I did the upgrade, it went well"
- **THEN** the next question is marked as pushing back and asks who decided, how many people worked on it and how long it took, and no fact is proposed

#### Scenario: A real answer
- **WHEN** the person answers "I proposed it and led it. 2 others helped part time. It took 7 months."
- **THEN** they are shown short facts, each with their exact words, with Keep, Fix and Drop, and the areas that answer covered go up

#### Scenario: A contradiction
- **WHEN** an answer says 12 months where an earlier fact says 7
- **THEN** the next question asks which is right, and pushes back

#### Scenario: Skip and done
- **WHEN** the person skips a question
- **THEN** that area is left alone until they start the role again, and the next question comes; and **WHEN** they choose Done for now, nothing more is asked and no model is called

#### Scenario: Coming back
- **WHEN** the person opens a role with a question waiting
- **THEN** the same question is shown and no model is called; and a role with all seven areas covered asks nothing more

### Requirement: Facts carry the person's own words
A fact SHALL be proposed only if its quote is a verbatim part of the
person's answer (ignoring case, spacing, and typographic quotes and dashes)
and its text says no number, name or tool that neither the quote nor the
profile says. A fact that fails either check SHALL be left out, counted and
never shown. Coverage SHALL only ever go up. A kept fact SHALL be saved on
that role in the profile, with its quote, as evidence for every later check.

#### Scenario: A made-up quote
- **WHEN** the model proposes a fact whose quote the person never wrote
- **THEN** it is left out and the person is told one line was left out

#### Scenario: A claim nobody made
- **WHEN** the model proposes "Took 9 months" with the quote "It took 7 months"
- **THEN** it is left out

#### Scenario: Fix
- **WHEN** the person fixes a fact to "Two others helped part time"
- **THEN** it is kept with that text and the original quote; and **WHEN** they write a number their words and profile do not say, keeping is refused naming the words and nothing is saved

#### Scenario: Keep all
- **WHEN** the person chooses Keep all with three facts waiting
- **THEN** all three are saved on the role as one profile version

#### Scenario: Remove and undo
- **WHEN** the person removes a kept fact from the profile and chooses Undo
- **THEN** it is back on the role, with its quote

#### Scenario: Waiting too long
- **WHEN** a fact has waited 24 hours without Keep, Fix or Drop
- **THEN** it is deleted and can no longer be kept

### Requirement: Grill me is small, capped and never in the background
Each start, answer or skip SHALL make at most one model call, and no call
SHALL be made in the background. Calls SHALL go through the account and
instance spending limits, which are checked before each call. An account
SHALL be limited to 150 Grill me calls per UTC day, refused with
`grill_daily_cap` before any call. Without a model, or over a limit, the page
SHALL say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 150 Grill me calls today and answers
- **THEN** the answer is refused with `grill_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and the answer typed stays in the box

#### Scenario: No model
- **WHEN** no AI model is configured
- **THEN** the roles list still opens, and starting an interview says AI is not available

### Requirement: Interviews belong to the role they were started for
An interview SHALL be tied to the position it was started for, so that
editing, removing or reordering positions never shows one role's interview,
or keeps one role's facts, on another.

#### Scenario: A position moved
- **WHEN** the person's positions are reordered after an interview
- **THEN** the position now at that place shows as not started
