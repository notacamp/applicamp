## ADDED Requirements

### Requirement: The retired Improve, draft, suggested-skills and Grill me interview endpoints are gone
The API SHALL NOT offer the AI profile review and its questions
(`/api/v1/profile/questions*`), drafts made from an answer
(`/api/v1/profile/drafts/*`), suggested skills (`/api/v1/profile/skills/suggested*`)
or Grill me's interview (`GET /api/v1/profile/grill` and
`/api/v1/profile/grill/{position}/start|answer|skip|done`), and no background job
SHALL run a profile review. Their tables SHALL NOT exist. What is left of Grill me is
the facts taken from an answer about a job and their controls
(`GET /api/v1/profile/grill/{position}`, `POST /api/v1/profile/grill/proposals/{id}/keep|drop`,
`POST /api/v1/profile/grill/{position}/keep-all`), and the order of the skills
(`PUT /api/v1/profile/skills/order`).

#### Scenario: A retired endpoint
- **WHEN** a request is made to a retired endpoint
- **THEN** it is not found, and nothing is read, written or asked of a model

#### Scenario: The facts from a job's question survive
- **WHEN** the person answers a question that comes from a job
- **THEN** the facts wait on the position and can be kept, fixed or dropped, as before

## MODIFIED Requirements

### Requirement: An answer about a job becomes facts on a position
An answer to a question that comes from a job SHALL be read with one small model
call and checks: a fact's quote SHALL be
a verbatim part of the answer, and its text SHALL say no number, name or tool
that neither the quote nor the profile says; what fails SHALL be left out and
counted. The question SHALL be context for the model, never evidence. The
facts SHALL wait on the position the question names, or on the most recent
position when it names none (and the person SHALL be told), and SHALL be kept,
fixed or dropped with the facts' own controls. Nothing SHALL reach the profile
until the person keeps it.

#### Scenario: A question that names no position
- **WHEN** the person answers a question that is about no particular position
- **THEN** the facts wait on their most recent position, the page says so, and Keep, Fix and Drop work

#### Scenario: More than the answer says
- **WHEN** the model adds a number the answer never wrote
- **THEN** that fact is left out and counted

### Requirement: A CV can add a fact or a skill in the person's own words
When the person says a line of a tailored CV is true, the profile SHALL keep
their words as a fact on the position the line is about, with those words as its
quote and an area (`results` by default), as a new profile version, as keeping a
proposed fact does. The position SHALL still be the one the CV was made from, else it is
refused (`profile_changed`). Adding a skill SHALL add it at the end of the skills
unless the profile already lists it under another spelling, and SHALL forget an
earlier "not mine" for it.

#### Scenario: A fact from a CV
- **WHEN** someone says a blocked line is true and writes it in their own words
- **THEN** the words are a kept fact on that position and appear under it on the profile

#### Scenario: The position changed
- **WHEN** the position was renamed or removed since the draft was made
- **THEN** nothing is added and the person is told to make the CV again

### Requirement: Review, then save everything at once
The page SHALL show every pending line grouped as Summary, Headline, Skills,
each position, Background and Goals, each marked "You told me" or "Confirmed",
with Edit (checked again against their words) and Remove. Save SHALL write all
changes and all confirmed assumptions in one profile write (and one goals write
when the goals changed), adding one profile version so the profile strength
refreshes. An assumption the person did not confirm SHALL NOT be
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
