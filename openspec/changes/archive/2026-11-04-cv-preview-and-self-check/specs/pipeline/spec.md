## ADDED Requirements

### Requirement: Every CV draft is a PDF you can see
Every draft, made by a click or by the preparation of a job, SHALL be rendered as a PDF as soon as it
is made and again after every change to it, inside the server with no model call: Clean on 1 page,
else the same template on 2 pages, else Compact on 2 pages, the first that fits (after a change, the
layout the person chose is tried first). What the read-back finds SHALL be kept as a note and SHALL
never refuse the preview. A render that fails SHALL be kept as a short code (`cv_doesnt_fit`,
`cv_pdf_unavailable`), SHALL never fail the change that asked for it, and SHALL be logged with its
cause (its kind and the compiler's counts, never the CV's text). The person SHALL be able to render
another template or length (no model call; refused with `cv_doesnt_fit` when it does not fit, the
preview kept as it was), see the preview inline and download it as "<Name> CV - <Company>
(draft).pdf", never cached or sniffed. When the PDF was made before the draft's last change, the
preview SHALL say it is stale and the page SHALL offer "Make the PDF"; a draft with no PDF SHALL offer
the same, never an endless "Making the PDF…". The person SHALL ask for at most 30 previews a day
(`cv_previews_daily_cap`); the ones made after a change are not counted. Another account's draft SHALL
be not found.

#### Scenario: A draft
- **WHEN** someone drafts a CV
- **THEN** the CV says its preview is ready on 1 page in the Clean template, and `…/drafts/1/preview.pdf` serves it as `application/pdf`

#### Scenario: Too long for one page
- **WHEN** a change leaves the CV too long for 1 page
- **THEN** the preview is made on 2 pages, and asking for 1 page is refused with `cv_doesnt_fit`

#### Scenario: Another layout
- **WHEN** the person picks Compact on 2 pages
- **THEN** the preview is made again in that layout and no model is called

#### Scenario: Someone else's draft
- **WHEN** another account asks for the preview's PDF
- **THEN** the answer is 404

#### Scenario: Older than the last change
- **WHEN** the draft's text changed and its PDF was not made again
- **THEN** the preview says it is stale, the page says "The PDF is older than your last change" with Make the PDF, and Approve renders the text anew

### Requirement: The app fixes what the check flags before asking
After the check, when a line is flagged, the app SHALL make one more model call for all the flagged
lines (never a skill), showing each line, why it was flagged and the sources it may use (under a
position only that position's and the skills; a company fact only in the summary, marked as a fact
about the company, not the person) as data, never the person's name or how to reach them. For each line it SHALL get either a rewrite that says only what those sources say, which
SHALL replace the line only when it passes the check (marked as fixed by the app, with what it said
before kept), or one yes-or-no question with the claim as a first-person fact, kept as the line's
proposal only when that fact backs the line and claims nothing that neither the line nor its own
sources (never the company's) say (else "Yes, keep it" keeps the line's own words). A company fact
used as an achievement, a summary whose flagged words only the company's facts say, and another job's
line SHALL never be proposed for keeping. There SHALL be one round
only, through the same caps and spending limits; a call that cannot be made, or answers nothing
usable, SHALL leave every flagged line to the person. Nothing SHALL reach the profile in this step.

#### Scenario: A rewrite that holds
- **WHEN** the draft says "20 minutes" where the highlight it cites says 40, and the app rewrites it to what the highlight says
- **THEN** the line is the rewrite, marked as fixed by the app with its first words kept, and is no longer flagged

#### Scenario: A rewrite that still says too much
- **WHEN** the app's rewrite still names a number its sources do not say
- **THEN** the line keeps its words and waits on the person with the app's question

#### Scenario: The model cannot answer
- **WHEN** the fix call fails or answers nothing usable
- **THEN** the draft is kept as checked and every flagged line waits on the person

## MODIFIED Requirements

### Requirement: A line may say only what its sources say
Without a model, every line of a draft SHALL be checked against the sources it cites. The claims that
matter SHALL appear in the text of those sources (with the head of their positions, and the company's
name for a company fact): a number (with or without a unit, a currency or a percentage, a year), a
technology or a skill of the profile, the name of an employer (a company of the profile, or the
job's), an acronym, and a name the profile does not give. A word written with a capital that names no
fact (Developer, Stack, a month) SHALL not be a claim; a rank or a seniority (Senior, Lead, Staff,
Head, Principal, Founder, Manager and the like) SHALL be a claim unless a position's title says it; a
place or a language the profile names SHALL be allowed in any line; the summary MAY use the posting's
words and acronyms, never its numbers, technologies, employers or ranks; the headline MAY say what any
position's title says. Places, languages and the posting's words SHALL match whole words only. A
source id that does not exist SHALL count as no source. A bullet under a position SHALL be backed only
by that position's sources and the skills: another position's source SHALL be flagged ("This line
belongs to another job") and SHALL not back it, and the profile's headline, summary, education and
languages SHALL not back it. A company fact cited anywhere but the summary SHALL be flagged ("company
facts can't be your achievements"). Every skill SHALL be one the profile lists, with aliases matched as
one skill. A line that fails SHALL be blocked and carry the reason, naming the claim, the sources
cited, the other position's sources, the company fact or the skill.

#### Scenario: A number nobody wrote
- **WHEN** a line cites a highlight that says 40 minutes and says 20 minutes
- **THEN** it is blocked, and the reason names the claim 20 and the source it cites

#### Scenario: Words that name no fact
- **WHEN** the headline says "Senior Full Stack Developer" citing the skills, a position's title says Senior, and the summary says the person is based in Lisbon, which the profile's location says
- **THEN** neither is blocked

#### Scenario: A rank nobody holds
- **WHEN** the headline says "Team Lead" and no position's title says Lead, even though the posting does
- **THEN** it is blocked, and the reason names Lead

#### Scenario: Part of a word
- **WHEN** the profile's location is San Francisco and a bullet names Cisco
- **THEN** it is blocked, and the reason names Cisco

#### Scenario: A tool nobody wrote
- **WHEN** a bullet says Kubernetes and no source it cites does, even though the posting asks for it
- **THEN** it is blocked, and the reason names Kubernetes

#### Scenario: Another job's line
- **WHEN** a bullet under the first position cites a highlight of the second
- **THEN** it is blocked with the reason that it belongs to another job

#### Scenario: A company fact as an achievement
- **WHEN** a bullet cites a company fact
- **THEN** it is blocked with the reason that company facts can't be the person's achievements, while the summary may cite one

#### Scenario: A skill off the profile
- **WHEN** the skills list holds Kubernetes and the profile lists no such skill
- **THEN** it is blocked, while a skill written differently from one on the profile (Postgres for PostgreSQL) passes

### Requirement: A blocked line is fixed, said to be true, or removed
What the app could not fix SHALL wait on the person as a question: "Is this true?" with the line,
the app's own question when it asked one, the person's own words it was made from, from the position
the card names ("You told us about <company>: …"), and what a yes keeps word for word ("If you keep
it, this goes on your profile: …"), and Yes, keep it / Change it / Remove it. When there is no yes,
the card SHALL say why: it is about the company, it is another job's line, or it is too long to keep
as a fact. Change it SHALL start from the person's own
words, never the model's, and an empty line SHALL not be saved. "Yes, keep it" SHALL keep the
app's proposed fact (else the line's own words) on the line's position of the profile as a confirmed
assumption based on "The CV draft for <company>", after which the line SHALL be checked again with
that fact as a source and without another position's or a company's sources. It SHALL be refused
(`cannot_keep`) for a company fact used as an achievement, a summary whose flagged words only the
company says, another job's line, and a line longer than a fact. Change it SHALL edit the line, the new text
being checked against the same sources (a rewritten line no longer cites company facts unless it is
the summary, nor another position's sources under a position). The person MAY still say it is true in
their own words, which SHALL be saved as a fact on the line's position with those words as its quote
(area `results` unless they choose another). For a blocked skill, Yes SHALL add the skill to the
profile. None of these SHALL call a model; each SHALL make the preview again.

#### Scenario: Yes, keep it
- **WHEN** a line is blocked for "a team of 6" and the app proposed "I led a team of 6 engineers."
- **THEN** the card shows "If you keep it, this goes on your profile: “I led a team of 6 engineers.”", and Yes keeps that sentence on the position as a confirmed assumption, the line cites it and passes

#### Scenario: A fact that says more
- **WHEN** the app proposes a fact that names a tool, a number or a name neither the line nor its own sources say
- **THEN** it is not proposed, and Yes keeps the line's own words

#### Scenario: It's true
- **WHEN** a line is blocked for a number and the person writes the number in their own words
- **THEN** the words are a kept fact on that position of their profile, the line cites it and passes

#### Scenario: Edit
- **WHEN** the person rewrites a blocked line to say only what its sources say
- **THEN** it passes and is marked as edited

#### Scenario: A company fact
- **WHEN** the person says yes to a line that uses a company fact as an achievement
- **THEN** it is refused with `cannot_keep` and nothing reaches the profile

### Requirement: Approving a CV makes a PDF that is read back
Approving a draft SHALL render its text as a PDF inside the server with the template (Clean or
Compact) and length (1 or 2 pages) asked for, the preview's when left out, else Clean on 1 page, with
the standard section headings in the CV's language, real text, real links and embedded fonts. When
the preview was made from the draft's text as it is now, in that layout, and its read-back passed, its
PDF SHALL be kept as it is; another text or another layout SHALL be rendered anew. The person's text SHALL be passed as data and never interpreted as markup. The
PDF's text SHALL then be read back with no AI and SHALL contain every section heading, every line,
every job keyword the review showed as on the CV, the name, the email and the links, within the page
count chosen. If anything fails, nothing SHALL be approved and the reasons SHALL be shown. A draft
approved without a PDF SHALL offer to make it. For 15 seconds after approving, Undo SHALL put the
draft back as a draft, its PDF version gone, and the page SHALL say what starts next by itself and
what it costs; after that, or once the cover letter has started, it SHALL be refused (`too_late`).

#### Scenario: Approved with a PDF
- **WHEN** someone approves a CV with nothing blocked
- **THEN** the draft becomes version n with a PDF, and its read-back results are kept with it

#### Scenario: What they saw
- **WHEN** someone approves a draft whose preview was made from its text as it is now
- **THEN** the version's PDF is the preview's, byte for byte

#### Scenario: Text that looks like markup
- **WHEN** a line contains `#import`, `]`, `$`, `*`, `_`, `<x>` or `@ref`
- **THEN** it appears literally in the PDF's text

#### Scenario: It does not fit
- **WHEN** the CV needs 2 pages and 1 was chosen
- **THEN** approving is refused with "Doesn't fit on 1 page: switch to 2 pages or remove a few lines" and nothing is approved

#### Scenario: The PDF loses something
- **WHEN** the read-back cannot find a heading or a line
- **THEN** nothing is approved and the missing items are listed

#### Scenario: Undo
- **WHEN** someone approves a CV and presses Undo within 15 seconds
- **THEN** it is a draft again with no version, and the cover letter that was to start next waits for the CV again

#### Scenario: Undo, then approve again
- **WHEN** someone approves, undoes and approves again within 15 seconds
- **THEN** the cover letter does not start while the second approval can still be undone, and starts after

#### Scenario: Approved before PDFs
- **WHEN** a draft was approved and has no PDF
- **THEN** the CV says so and making the PDF makes its first version

### Requirement: The app shows the PDF and lists every CV
The review page SHALL show the draft's PDF as its main content, made again after every change, with
its pages and language in one line and, when the read-back noted something, one plain sentence; beside
it the questions that wait on the person, "Checked every line against your profile" with how many
lines the app fixed (each such line marked "Fixed by the app"), the layout (Clean or Compact, 1 or 2 pages), Approve and Download draft. The app
SHALL open a PDF in a viewer (full screen on a phone, a large dialog on a desktop) that draws every
page, offers Download and closes with Escape, with PDF.js loaded only when a PDF is first shown and
from the app's own files. Once approved, the page SHALL offer Open the PDF, the read-back results,
template and length controls (a change on an approved CV asks first and says no AI is used) and the
versions. The peek SHALL offer View PDF and Download, and a Cover letter line (draft or approved, with
Review and View PDF). A CVs page SHALL list every version and draft of CVs and of cover letters with
All, CVs, Letters and Drafts filters.

#### Scenario: The review
- **WHEN** someone opens the review of a draft
- **THEN** its PDF is drawn on the page, and each question is a card with Yes, keep it, Change it and Remove it

#### Scenario: The CVs page
- **WHEN** someone has v1 and v2 of one CV, a cover letter v1 and a draft for another job
- **THEN** /cvs lists all four, newest first, View PDF opens a version, and Letters shows only the letter

### Requirement: Drafts are capped, expire and never run in the background
An opportunity SHALL have at most 10 drafts and an account at most 20 drafts per
UTC day, refused with `cv_drafts_per_opportunity_cap` and `cv_drafts_daily_cap`
before any call; drafts made by a job's preparation count the same. A draft
SHALL start only from a click or from the preparation of a job the person
shortlisted, and SHALL make at most two model calls: the draft, and the fix of
what the check flagged; its cost estimate SHALL count both at their worst. The calls SHALL go through the account and instance
spending limits, and nothing SHALL be retried past a limit or run on a timer.
Rendering a preview SHALL call no model. A draft nobody approved SHALL be
deleted 7 days after its last change; approved versions SHALL be kept.

#### Scenario: Over the daily cap
- **WHEN** an account has made 20 drafts today and drafts another
- **THEN** it is refused with `cv_drafts_daily_cap` and the model is not called

#### Scenario: Left alone
- **WHEN** a draft is not approved and not changed for 7 days
- **THEN** it is gone, while an approved version stays

### Requirement: A shortlisted job is prepared by itself, waiting only on the person
When an opportunity moves from Found to Shortlisted or later, and the person's Autonomy goal is
"Prepare" or not answered, the app SHALL by itself read the company and compare the job, then
stop while a question of the brief is open, then draft the CV. Approving the CV SHALL draft the
cover letter and the job's answers by itself, once the 15 seconds in which the approval can be
undone have passed. Opening Prepare on a Shortlisted or later
opportunity that was never prepared SHALL start the same chain. Answering questions, keeping
facts, approving the CV and the letter, and every stage move SHALL still wait on the person.
With the goal "Find only", or without a model, nothing SHALL run by itself and every step SHALL
start with a click, as before. A step SHALL never run twice for the same opportunity at once.

#### Scenario: Shortlisting a job
- **WHEN** someone with the goal "Prepare" moves a job to Shortlisted, its company's site is known and the comparison asks no question
- **THEN** the company is read, the job compared and a CV drafted without another click, and the CV waits for their approval

#### Scenario: A question is open
- **WHEN** the comparison asks a question
- **THEN** no CV is drafted until the person answers or skips every question, and the job shows that it waits on them

#### Scenario: Approving the CV
- **WHEN** the person approves the CV of a job being prepared by itself
- **THEN** the cover letter and the job's answers are drafted once Undo has passed and wait for their approval

#### Scenario: Find only
- **WHEN** someone whose goal is "Find only" shortlists a job
- **THEN** nothing is fetched and no model is called until they click

#### Scenario: Jobs shortlisted earlier
- **WHEN** someone with the goal "Prepare" opens Prepare on a Shortlisted job that was never prepared
- **THEN** the chain starts
