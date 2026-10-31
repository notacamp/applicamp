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
- **WHEN** the person confirms the assumption "I assume you led a small team at Example Co, probably 3 to 6 people" and saves
- **THEN** the fact is its plain claim ("Led a small team at Example Co (3 to 6 people)", shown before saving, checked against the sentence's own text and falling back to the sentence without "I assume"), on that position, marked as a confirmed assumption with the reason it was assumed, shown as such on the profile, and used as evidence like any other fact

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
The portrait SHALL make one model call per profile version, each employer read one, each "Anything else" one, each question shown one and each answer one; no call SHALL be made in the background and settling, editing, removing, skipping, stopping, reviewing, reloading a question already on screen and saving SHALL make none. Calls SHALL go through the account and instance spending limits, checked before each call. An account SHALL be limited to 60 wizard calls per UTC day, refused with `about_daily_cap` before any call. Without a model, or over a limit, the page SHALL say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 60 wizard calls today and asks for a question
- **THEN** it is refused with `about_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and what the person wrote stays in the box

#### Scenario: A question already on screen
- **WHEN** the page is reloaded with a question showing
- **THEN** the same question is shown and no model is called

### Requirement: Tell me about you asks about the gaps, one at a time
After "Anything else", Tell me about you SHALL ask questions about what the app does not know or only assumes, one at a time, each skippable, and the person SHALL be able to stop at any point with "That's enough for now". What is asked SHALL be decided in code from the profile, the kept facts, the confirmed and wrong assumptions, the goals and the wizard's state, and never by a model. The gaps SHALL be ranked: assumptions the person left for later (and, after "Not really", those they called wrong), then the most recent role's results without a number, then empty skills, then the rest: a lead or manager title without a team size, results without a number on the other recent roles, technologies the roles name that are not skills, a skill the person told that none of their roles mention, an empty summary, headline or background, goals not answered, and each area of each recent role nothing has been kept about. What the person has told so far SHALL close the gap it answers. The company's context SHALL never be asked. Questions SHALL stop when no gap is left, after 12 (8 after "Yes, mostly", 16 after "Not really"), or when the person says so. A skipped question SHALL not come back for the same profile version. Each question SHALL show how many have been asked of about how many, and the profile's strength with "→ N% if you answer", computed from the gap's weight in the strength.

#### Scenario: An assumption left for later is asked first
- **WHEN** the person chose Ask me later on "I assume you shipped pipelines" and continues
- **THEN** the first question is about it, saying what was assumed and why

#### Scenario: A role without a number
- **WHEN** the most recent role states no result with a number
- **THEN** a question about one result with a number comes next, tagged I don't know, with the strength it would add

#### Scenario: A lead title without a team size
- **WHEN** a position is titled Lead Engineer and nothing is kept about leading people
- **THEN** a question about the team size is asked, with buckets to pick from

#### Scenario: Skills the roles name
- **WHEN** the roles mention Kafka and the skills do not
- **THEN** the question says "Your roles mention Kafka, but it isn't a skill" and offers Kafka as a choice

#### Scenario: A claim no role backs
- **WHEN** the person wrote that they use Elixir and none of their roles mention it
- **THEN** a question tagged You told me asks where they used it

#### Scenario: Context is not asked
- **WHEN** the company's website was read, or the portrait assumed what the company does
- **THEN** no question asks what the company sells; at most the assumption is asked to be confirmed

#### Scenario: Skip and enough
- **WHEN** the person skips a question
- **THEN** it is not asked again for this profile version, no model is called, and the next comes; and **WHEN** they choose That's enough for now, no more is asked and they go to the review

#### Scenario: The list runs out
- **WHEN** no gap is left, or 12 questions have been answered or skipped
- **THEN** the person goes to the review

#### Scenario: Not really
- **WHEN** the person answered "Not really" to "Is this accurate?"
- **THEN** up to 16 questions are asked, and the assumptions they marked wrong are asked about too

### Requirement: Every question says why it is asked
A question SHALL carry a reason that says what Applicamp assumed or does not know about the person, a tag (I assumed, I don't know, You told me), and, only for a closed question, choices. A question SHALL never come without a reason (the gap's own basis stands in when the model leaves it out). The model SHALL be shown only the top gap and its context (that position's entry and facts, and the related assumption), never the whole profile and never a name, email, phone, address or link; it SHALL be told never to ask for anything it knows or that can be looked up, and to state what it knows as an assumption to confirm.

#### Scenario: A reason that names the assumption
- **WHEN** a position is titled Lead Engineer and its team size is not in the profile
- **THEN** the question says, in the person's language, that the title usually means leading 3 to 6 engineers, that the profile does not say, and that recruiters ask

#### Scenario: Only the gap's context
- **WHEN** a question is worded about the second position
- **THEN** the model sees that position's words and facts and not the first position's, and no contact detail

### Requirement: An answer becomes changes, flagged and never dropped
An answer, in the person's words and the choices they picked (which SHALL be among those offered), SHALL be read with one model call into proposed changes with the same structure and checks as "Anything else": each quotes their words, a quote that is not theirs and any claim the value adds that their words do not say SHALL be a flag, shown and never dropped, settled with It's true, Fix or Use my own words. A fact SHALL belong to the position the question was about. The changes SHALL join the pending changes (a newer summary, headline or background replaces an older one; a skill already there is not added twice), the portrait SHALL gain a told sentence for each, marked as new, and the screen SHALL say where each change goes. A new "Anything else" SHALL replace only the changes it made. Nothing SHALL be saved until Save.

#### Scenario: A number
- **WHEN** the person answers a results question with "runs went from 6 hours to 40 minutes"
- **THEN** a change is proposed on that position with their words as its quote, the portrait gains the sentence as new, the question is closed, and the profile is unchanged until Save

#### Scenario: A claim that is not theirs
- **WHEN** the model's change names a company the answer never mentions
- **THEN** the change is kept with a flag naming the company, and Save waits until it is settled

#### Scenario: A choice the question did not offer
- **WHEN** the request names a choice that is not among those shown
- **THEN** it is refused and nothing is read

### Requirement: Improve my profile, Skills and Grill me are retired into Tell me about you
The routes `/profile/improve`, `/profile/skills` and `/profile/grill` (and `/profile/grill/{position}`) SHALL lead to Tell me about you. Every entry point to them SHALL lead there instead: the Grill me button, the per-role "Grill me about this role" links, Today's nudge and the strength checklist's Add and Improve buttons for results, skills and questions (a part with a place on the profile, such as the headline or a link, is still added in place). The facts the person kept SHALL stay on the profile under "What you told Applicamp about each role", and the skills SHALL be reordered from the profile's own skills list. Skills the roles name that the skills lack SHALL be a question, not a page.

#### Scenario: An old address
- **WHEN** someone opens `/profile/grill` or `/profile/improve`
- **THEN** they land on Tell me about you

#### Scenario: A role's kept facts
- **WHEN** the person opens Profile & goals
- **THEN** each role lists the facts they told Applicamp about it, with their own words and Remove, under "What you told Applicamp about each role", and no Grill me button

#### Scenario: Skills in order
- **WHEN** the person moves a skill up in the profile's skills list
- **THEN** it is listed where they put it, the first three are marked as the ones the job board searches use, and nothing else changes

### Requirement: The profile's strength leads to Tell me about you
The app SHALL show how strong the profile is and what is missing, and lead the person to Tell me about you as the way to make it stronger, which asks about the gaps one at a time. A fact the person kept about a position's results SHALL count toward the strength like a highlight. Nothing SHALL be saved to the profile until they save, and the flow SHALL never block the rest of the app.

#### Scenario: Skills already in the roles
- **WHEN** "Rust" appears in four position descriptions but not in the skills list
- **THEN** Tell me about you asks about it ("Your roles mention Rust, but it isn't a skill") and adds it only when the person says so and saves

#### Scenario: Skills in order
- **WHEN** the person moves a skill up in the profile's skills list
- **THEN** it is listed where they put it, the first three are marked as the ones the job board searches use, and nothing is added to or removed from the list by the move

#### Scenario: A result raises the strength
- **WHEN** the person answers a results question with a number and saves
- **THEN** the profile strength rises by what the question said

#### Scenario: Not now
- **WHEN** the person chooses "Not now" on Today's profile strength card
- **THEN** it is hidden for a week, and every other part of the app works as before

#### Scenario: The way in
- **WHEN** the person opens Profile & goals, or follows Today's profile card, or chooses a part of the checklist that needs more from them
- **THEN** they arrive at Tell me about you
