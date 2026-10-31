## MODIFIED Requirements

### Requirement: Prepare a job on real facts about the company
From an opportunity at Shortlisted or later, the app SHALL offer a page that
prepares a tailored CV's facts: what the company's own pages say, what the job
wants most, and the few questions where the person's profile is thin. Opening
the page SHALL make no model call and fetch nothing; every step SHALL start
with a click. The page SHALL offer "Draft the CV" with what it costs, or
"Review the CV" when a draft exists, and say why drafting is off when it is.

#### Scenario: Opening it
- **WHEN** someone opens "Prepare: company and job" on a Shortlisted opportunity
- **THEN** they see the company's site, a Read button with what it costs, a Draft the CV button with what it costs, and nothing has been fetched or asked of the model

#### Scenario: Found is too early
- **WHEN** an opportunity is still Found
- **THEN** its peek does not offer the Tailored CV block, and drafting a CV for it is refused with `stage_too_early`

## ADDED Requirements

### Requirement: Draft a CV from the profile, every line citing its sources
From a Shortlisted or later opportunity, one click SHALL make a draft CV with
one model call: a headline, a summary, positions with bullets and skills, each
text citing the ids of the profile sources it comes from (a position's summary,
a highlight, a kept fact, the summary, the headline, the skills list,
education, languages) and, in the summary only, the company facts the person
has not removed. The model SHALL be shown the posting, what the job wants, the
company facts and the profile as data, and SHALL never be shown the person's
name, email, address, links or any contact detail: the server SHALL print those
itself. The draft SHALL be written in the language asked for, else the
posting's, else English.

#### Scenario: A draft
- **WHEN** someone drafts a CV for a Shortlisted opportunity
- **THEN** the draft has a number, a language and lines that each cite sources, the contact is the profile's, and the prompt sent to the model holds no name or contact detail

#### Scenario: Not Shortlisted yet
- **WHEN** someone drafts a CV for an opportunity still at Found
- **THEN** it is refused with `stage_too_early` and the model is not called

#### Scenario: Another person's opportunity
- **WHEN** someone drafts, reads or changes the CV of an opportunity that is not theirs
- **THEN** it is not found

### Requirement: A line may say only what its sources say
Without a model, every line of a draft SHALL be checked against the sources it
cites: each claim of the line (a number, a technology, an acronym, a name) SHALL
appear in the text of those sources (with the head of their positions, and the
company's name for a company fact). A source id that does not exist SHALL count
as no source. A company fact cited anywhere but the summary SHALL be flagged
("company facts can't be your achievements"). Every skill SHALL be one the
profile lists, with aliases matched as one skill. A line that fails SHALL be
blocked and carry the reason, naming the claim, the sources cited, the company
fact or the skill.

#### Scenario: A number nobody wrote
- **WHEN** a line cites a highlight that says 40 minutes and says 20 minutes
- **THEN** it is blocked, and the reason names the claim 20 and the source it cites

#### Scenario: A company fact as an achievement
- **WHEN** a bullet cites a company fact
- **THEN** it is blocked with the reason that company facts can't be the person's achievements, while the summary may cite one

#### Scenario: A skill off the profile
- **WHEN** the skills list holds Kubernetes and the profile lists no such skill
- **THEN** it is blocked, while a skill written differently from one on the profile (Postgres for PostgreSQL) passes

### Requirement: A blocked line is fixed, said to be true, or removed
For a blocked line the person SHALL be able to remove it; edit it, the new text
being checked against the same sources (a rewritten line no longer cites company
facts unless it is the summary); or say it is true in their own words, which
SHALL be saved as a fact on the line's position of the profile with those words
as its quote (area `results` unless they choose another), after which the line
SHALL be checked again with that fact as an extra source. For a blocked skill,
"I have it" SHALL add the skill to the profile. Saying it is true SHALL not be
offered for a company fact used as an achievement. None of these SHALL call a
model.

#### Scenario: It's true
- **WHEN** a line is blocked for a number and the person writes the number in their own words
- **THEN** the words are a kept fact on that position of their profile, the line cites it and passes

#### Scenario: Edit
- **WHEN** the person rewrites a blocked line to say only what its sources say
- **THEN** it passes and is marked as edited

### Requirement: Approve only when every line traces to the profile
Approve SHALL be refused (`lines_flagged`) while any line or skill is blocked,
the check being run again at that moment, and the page SHALL say how many
lines need the person and take them to the first. Approving SHALL freeze the
draft as the opportunity's next version; an approved or discarded draft SHALL
not change. Making the CV again SHALL be a new draft, and the earlier
unapproved one SHALL be put away.

#### Scenario: Blocked
- **WHEN** a line is still blocked
- **THEN** Approve is off and says when it turns on, and asking the server to approve is refused

#### Scenario: Approved
- **WHEN** the last blocked line is fixed and the person approves
- **THEN** the draft is version 1 of the opportunity, its contact is read from the profile again, and the peek says v1 approved

### Requirement: What the draft left out can be put back as the profile has it
The review SHALL list the positions and the lines of shown positions that the
draft left out, and "Put back" SHALL restore a position (up to three of its
highlights, else kept facts, else its summary) or one line word for word, each
citing its own source so it passes the check, without a model.

#### Scenario: Putting a role back
- **WHEN** the person puts back a position the draft left out
- **THEN** it returns in its place with its lines as the profile has them, and nothing is blocked because of it

### Requirement: The review shows how the CV relates to the profile and the job
Each line SHALL be marked Reworded, Moved up, As in profile or Edited, and a
reworded line SHALL open onto the words it was made from with the claims picked
out. Beside the CV the page SHALL show whether every line traces to the profile,
the job's keywords (what it wants, and the technologies the posting names) as on
the CV, in the profile but not on the CV, or not in the profile, with how many
are on the CV, and counts of what changed. Keywords the person lacks SHALL never
be added for them. On a narrow page the card SHALL come above the CV.

#### Scenario: Keywords
- **WHEN** the posting asks for a technology the profile does not list
- **THEN** it is shown as not in the profile and is not on the CV

### Requirement: Drafts are capped, expire and never run in the background
An opportunity SHALL have at most 10 drafts and an account at most 20 drafts per
UTC day, refused with `cv_drafts_per_opportunity_cap` and `cv_drafts_daily_cap`
before any call. The call SHALL go through the account and instance spending
limits, and nothing SHALL be retried past a limit or run on a timer. A draft
nobody approved SHALL be deleted 7 days after its last change; approved versions
SHALL be kept.

#### Scenario: Over the daily cap
- **WHEN** an account has made 20 drafts today and drafts another
- **THEN** it is refused with `cv_drafts_daily_cap` and the model is not called

#### Scenario: Left alone
- **WHEN** a draft is not approved and not changed for 7 days
- **THEN** it is gone, while an approved version stays
