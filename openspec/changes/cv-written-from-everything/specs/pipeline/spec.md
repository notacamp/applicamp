## ADDED Requirements

### Requirement: Jobs that fit well are proposed, and shortlisted by the app with "Prepare"
A job a search finds with a match of 75 or more, not ruled out by a requirement and open to the
person, SHALL be proposed to them, listed best match first; moving its stage or setting it aside
SHALL withdraw it. After a search, when the person's Autonomy goal is "Prepare" or not answered, the
app SHALL shortlist the best proposals by itself, at most three per UTC day per account, and start
their preparation; each SHALL say it was shortlisted by the app. It SHALL shortlist nothing with the
goal "Find only", without a model, when the day's CV drafts are used, or when a spending limit is
reached. No model SHALL be called to propose or to shortlist.

#### Scenario: A proposal
- **WHEN** a search finds a job with a match of 82 that is open to the person
- **THEN** it is listed among the proposals, until the person moves it or sets it aside

#### Scenario: Prepare
- **WHEN** a search ends for someone whose goal is "Prepare" and five jobs are proposed
- **THEN** the three best are shortlisted by the app and their preparation starts, and the next search that day shortlists none

#### Scenario: Find only
- **WHEN** a search ends for someone whose goal is "Find only"
- **THEN** the proposals stay Found

## MODIFIED Requirements

### Requirement: Draft a CV from the profile, every line citing its sources
From a Shortlisted or later opportunity, one click, or the preparation of a job
the person shortlisted, SHALL make a draft CV with one model call on the
`write` model (else `complete`): a headline of at most 12 words, a summary of
two or three sentences, positions with bullets and skills, each text citing the
ids of the profile sources it comes from (a position's summary, a highlight, a
kept fact, the summary, the headline, the background, a project, a
certificate, the skills list, education, languages) and, in the summary only,
the company facts the person has not removed. Every position with sourced text
SHALL be shown unless it is both of no help for the job and more than ten years
old, with 3 to 6 bullets for the two most recent and 1 to 3 for older ones;
positions SHALL be printed newest first whatever order the model gives, and the
ones left out SHALL be named in one "Earlier" line (title, company, years) by
the app. The model SHALL be shown the posting, what the job wants, the company
facts and the profile as data, and SHALL never be shown the person's name,
email, address, links or any contact detail: the server SHALL print those
itself. The draft SHALL be written in the language asked for, else the first
language of the person's goals, else one their profile says they speak well,
else English; the posting's language SHALL only be offered. When the job wants
something the profile shows and the draft does not say, one more call SHALL ask
for up to three bullets from the person's own sources, each checked like any
line.

#### Scenario: A draft
- **WHEN** someone drafts a CV for a Shortlisted opportunity
- **THEN** the draft has a number, a language and lines that each cite sources, the contact is the profile's, and the prompt sent to the model holds no name or contact detail

#### Scenario: Everything the profile knows
- **WHEN** the profile has a background, a project and a certificate
- **THEN** the model is shown them as sources it may cite

#### Scenario: Newest first, the rest named
- **WHEN** the model gives the older position first and leaves a third out
- **THEN** the CV shows the newest first and names the third under "Earlier" with its years

#### Scenario: The person's language
- **WHEN** someone who works in Portuguese drafts a CV for a posting in English
- **THEN** the CV is in Portuguese and English is offered

#### Scenario: What the job wants
- **WHEN** the job wants partner feeds, the profile shows them and the draft does not say it
- **THEN** one more call adds a bullet that does, citing the person's own line

#### Scenario: Not Shortlisted yet
- **WHEN** someone drafts a CV for an opportunity still at Found
- **THEN** it is refused with `stage_too_early` and the model is not called

#### Scenario: Another person's opportunity
- **WHEN** someone drafts, reads or changes the CV of an opportunity that is not theirs
- **THEN** it is not found

### Requirement: The review shows how the CV relates to the profile and the job
Each line SHALL be marked Reworded, Moved up, As in profile or Edited, and a
reworded line SHALL open onto the words it was made from with the claims picked
out. Beside the CV the page SHALL show what needs the person, what the job
wants (each on the CV, or not on the profile and so not claimed), what the app
did by itself and what it cost, the layout and the language, the job's keywords
as on the CV, in the profile but not on the CV, or not in the profile, and
counts of what changed. Keywords the person lacks SHALL never be added for
them. On a narrow page the card SHALL come above the CV.

#### Scenario: Keywords
- **WHEN** the posting asks for a technology the profile does not list
- **THEN** it is shown as not in the profile and is not on the CV

#### Scenario: What the job wants
- **WHEN** the job would like Elixir and the profile does not show it
- **THEN** the page says "Elixir · nice to have · not on your profile" and the CV does not claim it

### Requirement: Drafts are capped, expire and never run in the background
An opportunity SHALL have at most 10 drafts and an account at most 20 drafts per
UTC day, refused with `cv_drafts_per_opportunity_cap` and `cv_drafts_daily_cap`
before any call; drafts made by a job's preparation count the same. A draft
SHALL start only from a click or from the preparation of a job the person or
the app shortlisted, and SHALL make at most three model calls: the draft, the
second look at what the job wants, and the fix of what the check flagged. The
calls SHALL go through the account and instance spending limits, and nothing
SHALL be retried past a limit or run on a timer. Rendering a preview SHALL call
no model. A draft nobody approved SHALL be deleted 7 days after its last
change; approved versions SHALL be kept.

#### Scenario: Over the daily cap
- **WHEN** an account has made 20 drafts today and drafts another
- **THEN** it is refused with `cv_drafts_daily_cap` and the model is not called

#### Scenario: Left alone
- **WHEN** a draft is not approved and not changed for 7 days
- **THEN** it is gone, while an approved version stays
