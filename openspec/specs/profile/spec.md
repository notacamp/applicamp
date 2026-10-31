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
The person SHALL be able to see and edit every part of their profile,
including a background (where they come from: domains and earlier careers, in
their words), and an import SHALL be able to merge into it without losing what
they edited. The import and the model that structures it SHALL NOT write the
background.

#### Scenario: Merge keeps edits
- **WHEN** they merge a new import into a profile they edited by hand
- **THEN** their edited values stay and only new positions, schools and skills are added

#### Scenario: A background
- **WHEN** the person saves "6 years in events and restaurants" from Tell me about you
- **THEN** it shows on the profile as their background and can be edited in place like the summary

### Requirement: The profile keeps getting stronger
The app SHALL show how strong the profile is and what is missing, lead the
person to Tell me about you as the main way to make it stronger, ask the
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

#### Scenario: The way in
- **WHEN** the person opens Profile & goals
- **THEN** the strength card's main button is Tell me about you, in the place Improve my profile had, and the Improve page is still reachable from the checklist's parts

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
A fact SHALL be either something the person said in their own words or an
**assumption the person confirmed** in Tell me about you; the latter SHALL be
marked as such, SHALL keep what it was based on, and SHALL be written only when
the person saves.

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

#### Scenario: A confirmed assumption
- **WHEN** the person confirms the assumption "I assume you led a small team" and saves
- **THEN** it is a fact on that position marked as a confirmed assumption with the reason it was assumed, shown as such on the profile, and used as evidence like any other fact

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

### Requirement: An answer about a job becomes facts on a position
An answer to a question that comes from a job SHALL be read by Grill me's own
pipeline with one small model call and the same checks: a fact's quote SHALL be
a verbatim part of the answer, and its text SHALL say no number, name or tool
that neither the quote nor the profile says; what fails SHALL be left out and
counted. The question SHALL be context for the model, never evidence. The
facts SHALL wait on the position the question names, or on the most recent
position when it names none (and the person SHALL be told), and SHALL be kept,
fixed or dropped with Grill me's own controls. Nothing SHALL reach the profile
until the person keeps it.

#### Scenario: A question that names no position
- **WHEN** the person answers a question that is about no particular position
- **THEN** the facts wait on their most recent position, the page says so, and Keep, Fix and Drop work as in Grill me

#### Scenario: More than the answer says
- **WHEN** the model adds a number the answer never wrote
- **THEN** that fact is left out and counted

### Requirement: A CV can add a fact or a skill in the person's own words
When the person says a line of a tailored CV is true, the profile SHALL keep
their words as a fact on the position the line is about, with those words as its
quote and an area (`results` by default), as a new profile version, as Grill me's
Keep does. The position SHALL still be the one the CV was made from, else it is
refused (`profile_changed`). Adding a skill SHALL add it at the end of the skills
unless the profile already lists it under another spelling, and SHALL forget an
earlier "not mine" for it.

#### Scenario: A fact from a CV
- **WHEN** someone says a blocked line is true and writes it in their own words
- **THEN** the words are a kept fact on that position and appear under it on the profile

#### Scenario: The position changed
- **WHEN** the position was renamed or removed since the draft was made
- **THEN** nothing is added and the person is told to make the CV again

### Requirement: Tell me about you, a portrait to confirm
The app SHALL offer a page, "Tell me about you", that writes what it knows
about the person as one text built from their profile (without name, email,
phone, address or links), the facts and goals they kept, and what their latest
five past employers say about themselves on their own websites, and that marks
every sentence by where it comes from: from the profile, told by the person,
or assumed. Assumptions SHALL include what is usual for the person's roles
and SHALL each carry the reason they were assumed. The person SHALL settle each
assumption as right, wrong or "ask me later"; nothing SHALL be saved to the
profile until they save. A portrait SHALL be made once per profile version.

#### Scenario: The portrait
- **WHEN** a person with a profile opens Tell me about you
- **THEN** they see the portrait with sentences marked from your profile, you told me and I assumed, each assumption reachable by keyboard and opening its reason and three choices, and nothing is saved

#### Scenario: Role-typical suggestions
- **WHEN** the person's position is a Lead Engineer at a coliving company and the profile says only "backend work"
- **THEN** the portrait assumes they led a small team and worked on bookings, payments or building access, each with the reason it was assumed and what the profile does not say

#### Scenario: Right, wrong and later
- **WHEN** the person chooses That's right on one assumption, That's wrong on another and Ask me later on a third
- **THEN** the first shows as confirmed, the second is struck out and is not suggested again for this profile version, the third is kept for the questions that follow, and the profile is unchanged

#### Scenario: Once per version
- **WHEN** the person opens the page again for the same profile version
- **THEN** the same portrait is shown and no model is called; and **WHEN** the profile changed, a new portrait is made

#### Scenario: Never their contact details
- **WHEN** the portrait is made
- **THEN** the model is shown no name, email, phone, address or link

### Requirement: Nothing is claimed that the sources do not say
A sentence the model marks as from the profile or told by the person SHALL
pass the claim check against the sources it cites (every number, technology and
name SHALL appear in them) and, for told, SHALL cite only facts the person wrote
or confirmed. A sentence that fails SHALL be **downgraded to an assumption** with
the reason that it is not in the profile in these words, and SHALL NOT be dropped.
"Sourced" research SHALL need a quote and the page it was read on; the model's
general knowledge, including what is typical for a role, SHALL be an assumption.

#### Scenario: A claim the profile does not make
- **WHEN** the model writes "You run billing on Kubernetes" as from the profile and no cited source says Kubernetes
- **THEN** the sentence is shown as an assumption saying it is not in the profile in these words

#### Scenario: A sourced fact without a page
- **WHEN** employer research reports a fact as sourced with no quote or no page
- **THEN** it is treated as an assumption and is not offered to the portrait as a source

### Requirement: Past employers are read, not searched for
Researching a past employer SHALL reuse the company facts of the pipeline (one
site choice, one page reader with robots.txt and host guard, one quote-on-page
check and one 30-day cache per account and company, shared with a job's
company), through a port, for at most the five latest employers. It SHALL fetch
only when the employer's own site is known, SHALL never search the open web and
SHALL never loop. Page text SHALL be treated as data, never as instructions.

#### Scenario: A known site
- **WHEN** an employer's site is known and its facts are older than 30 days
- **THEN** its pages are read once, its facts are checked against their pages, and a rebuilt portrait within 30 days reads nothing again

#### Scenario: An unknown site
- **WHEN** an employer's site is not known
- **THEN** nothing is fetched, the page says so, and the person can type the site

#### Scenario: A site that cannot be read
- **WHEN** an employer's robots.txt forbids its home page
- **THEN** nothing is read for it, the page says why, and the portrait is still made

### Requirement: Is this accurate, and anything else
The page SHALL ask "Is this accurate?" (yes mostly, some of it's wrong, not
really) and store the answer. It SHALL ask "Anything else I should know about
you?" (up to 4,000 characters) and turn the text, with one model call, into
proposed changes across the profile: summary, headline, skills, background,
goals, and facts on a named position, each quoting the person's words. A claim
in a change that their words do not say SHALL be a flag naming the claim, shown
and never dropped, which the person settles as true, fixes (checked again) or
replaces with their own words, which puts their exact text in the field. The
changes SHALL add *told* sentences to the portrait, highlighted as new, and
nothing SHALL be saved yet.

#### Scenario: Changes across the profile
- **WHEN** the person writes that they ran events and a restaurant for 6 years, want a senior Rails role and use Rails, Elixir and Postgres
- **THEN** they are shown a summary, a headline, three skills, a background, a goal and each with their own words, and the portrait gains told sentences marked new

#### Scenario: A claim that is not theirs
- **WHEN** a proposed summary says they now work at a company their words never name
- **THEN** the change is shown with a flag naming that company, with It's true, Fix and Use my own words

#### Scenario: Use my own words
- **WHEN** the person chooses Use my own words on the flagged summary
- **THEN** the summary becomes their whole answer exactly as written and the flag is gone

#### Scenario: Skills are matched
- **WHEN** a proposed skill is "Postgres" and the profile has "PostgreSQL"
- **THEN** it is not saved as a second skill

### Requirement: Review, then save everything at once
The page SHALL show every pending line grouped as Summary, Headline, Skills,
each position, Background and Goals, each marked "You told me" or "Confirmed",
with Edit (checked again against their words) and Remove. Save SHALL write all
changes and all confirmed assumptions in one profile write (and one goals write
when the goals changed), adding one profile version so the profile strength and
the gap review refresh. An assumption the person did not confirm SHALL NOT be
saved. Save SHALL wait while a flag is unsettled. Pending state SHALL last seven
days.

#### Scenario: Save
- **WHEN** the person saves seven changes and one confirmed assumption
- **THEN** the profile gains one version, the summary, skills and background show on the profile, the confirmed assumption is a fact on its position marked as a confirmed assumption, and the pending state is gone

#### Scenario: Unconfirmed assumptions
- **WHEN** the person saves with two assumptions marked wrong or later and one left unanswered
- **THEN** none of the three is in the profile

#### Scenario: A flag waits
- **WHEN** a flagged change is not settled
- **THEN** Save is refused with `changes_flagged` and nothing is written

#### Scenario: The position moved
- **WHEN** a position was removed from the profile after a fact was proposed for it
- **THEN** that item is skipped and counted, and the rest is saved

### Requirement: Tell me about you is small, capped and never in the background
The portrait SHALL make one model call per profile version, each employer read
one, and each "Anything else" one; no call SHALL be made in the background and
settling, editing, removing, reviewing and saving SHALL make none. Calls SHALL go
through the account and instance spending limits, checked before each call. An
account SHALL be limited to 30 wizard calls per UTC day, refused with
`about_daily_cap` before any call. Without a model, or over a limit, the page SHALL
say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 30 wizard calls today and asks for a portrait
- **THEN** it is refused with `about_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and what the person wrote stays in the box
