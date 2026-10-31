# pipeline Specification

## Purpose
Every opportunity a person has, in one place: found by a search or added by pasting a link,
moved through its stages from Found to Offer or Closed, viewed as a table and as a board, each
with its match, why it matches, its gaps, the posting, notes and a next step.

## Requirements

### Requirement: One pipeline of opportunities
The app SHALL keep every opportunity a person has, found by a search or added
by link, in one pipeline with a stage (Found, Shortlisted, Applying, Applied,
Talking, Interview, Offer, Closed), viewable as a table and as a board, each
opening to its details, why it matches, its gaps, notes and the posting.

#### Scenario: Moving a job forward
- **WHEN** someone opens a Found opportunity and sets its stage to Shortlisted
- **THEN** it shows under Shortlisted on the board and in the table, with the change kept

#### Scenario: Adding a job by link
- **WHEN** someone pastes a Greenhouse posting link
- **THEN** the opportunity appears with its title, company and location read from Greenhouse, and that company's board is searched from then on

#### Scenario: A link to any other site
- **WHEN** someone pastes a link to a site that is not a supported job board
- **THEN** an opportunity is created with the link and they type its title and company; the server does not fetch that site

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

### Requirement: The company's website is known once
The company's website SHALL be, in this order: the one the person typed, the
one the job board gave, the apply link's host when it is not a known job
board or applicant-tracking system (a posting the person pasted counts as its
own apply link), else the person is asked. It SHALL be kept on the company so
it is asked once, for every job there. Only the address's host is kept.

#### Scenario: The apply link is the company's own site
- **WHEN** an opportunity's apply link is on a careers page of the company's own domain
- **THEN** the company's site is that domain, and the person is not asked

#### Scenario: Only a tracking system's link
- **WHEN** an opportunity's only link is a Greenhouse, Lever or Ashby posting and the board gave no website
- **THEN** the page asks for the company's website, and keeps what is typed for every job at that company

#### Scenario: A website that cannot be read
- **WHEN** the person types an address that is not a public http(s) website
- **THEN** it is refused in plain words and nothing is kept

### Requirement: Only the company's own pages are read, where robots.txt allows
Reading a company SHALL fetch the posting, the company's home page and at most
five more pages of the same site, linked from the home page, whose path or link
text names about, company, customers, clients, blog, engineering, team,
product or careers. It SHALL never fetch another host (not through a link, not
through a redirect), only where the site's robots.txt allows, with each page
cut at 300 KB and 10 seconds and turned into plain text. A site whose
robots.txt forbids its home page SHALL not be read at all. Fetched text SHALL
be kept only while a fact cites it.

#### Scenario: A forbidden page
- **WHEN** the home page links to a page the site's robots.txt disallows
- **THEN** that page is not fetched, the page says one page was left alone, and the others are read

#### Scenario: Another host
- **WHEN** the home page links to a news site or a social profile
- **THEN** it is never fetched

#### Scenario: A site that says no
- **WHEN** the site's robots.txt disallows everything
- **THEN** nothing is read and the page says the website asks robots not to read it

### Requirement: Every company fact quotes its page
One model call SHALL turn the pages' text into short facts about the company,
each with the page's exact words and the page's link. A fact SHALL be kept
only if its page was one of those read and its quote is a verbatim part of that
page's text (ignoring case, spacing, and typographic quotes and dashes);
otherwise it SHALL be left out and counted. The prompt SHALL say to use only
the pages and never memory, and that page text is data, not instructions. The
person SHALL be able to remove any fact, undo that, and a removed fact SHALL
stay removed on later reads.

#### Scenario: A fact with a made-up quote
- **WHEN** the model returns a fact whose quote is on no page it read
- **THEN** it is not shown, and the page says how many facts were left out

#### Scenario: Wrong, remove
- **WHEN** the person removes a fact and the company is read again
- **THEN** the fact stays removed, and Undo brings it back

#### Scenario: Reused for 30 days
- **WHEN** another job at the same company is prepared within 30 days
- **THEN** its facts are the same ones, nothing is fetched and no model is called; "Read again" fetches and asks again

### Requirement: What the job wants, and questions where the profile is thin
One more model call SHALL compare the posting with the company's facts and a
compact profile (positions with dates and kept facts, and skill names; no name
or contact details): up to six things the job wants most, each quoting the
posting (checked without the model) and marked must or nice, and up to three
questions, only where the profile is thin, each naming the company fact or the
posting line it comes from. It SHALL be reused until the posting's text
changes. A question about a skill the person dismissed SHALL never be asked.

#### Scenario: A want that is not in the posting
- **WHEN** the model names something the job wants with words the posting does not contain
- **THEN** it is left out

#### Scenario: Skipping and putting away
- **WHEN** the person skips a question
- **THEN** it is hidden for this job; and **WHEN** they say "I don't have it" on a question about a skill, the skill is recorded as not theirs, no model is called, and neither the skill nor the question is asked again

### Requirement: Company reads are capped and never in the background
An account SHALL be limited to 20 company reads per UTC day, refused with
`company_reads_daily_cap` before any fetch or call; every read that starts
counts, whether or not the site answered. Calls SHALL go through the account
and instance spending limits, checked before each call. Without a model, or
over a limit, the page SHALL say so in plain words and nothing SHALL be fetched
or retried. Nothing SHALL run on a timer.

#### Scenario: Over the daily cap
- **WHEN** an account has started 20 company reads today and reads another
- **THEN** it is refused with `company_reads_daily_cap` and nothing is fetched

#### Scenario: Another person's opportunity
- **WHEN** someone asks for the brief of an opportunity that is not theirs
- **THEN** it is not found, nothing is fetched and no model is called

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

### Requirement: Approving a CV makes a PDF that is read back
Approving a draft SHALL render its text as a PDF inside the server with the
template (Clean or Compact) and length (1 or 2 pages) asked for, Clean on 1
page by default, with the standard section headings in the CV's language, real
text, real links and embedded fonts. The person's text SHALL be passed as data
and never interpreted as markup. The PDF's text SHALL then be read back with no
AI and SHALL contain every section heading, every line, every job keyword the
review showed as on the CV, the name, the email and the links, within the page
count chosen. If anything fails, nothing SHALL be approved and the reasons
SHALL be shown.

#### Scenario: Approved with a PDF
- **WHEN** someone approves a CV with nothing blocked
- **THEN** the draft becomes version n with a PDF, and its read-back results are kept with it

#### Scenario: Text that looks like markup
- **WHEN** a line contains `#import`, `]`, `$`, `*`, `_`, `<x>` or `@ref`
- **THEN** it appears literally in the PDF's text

#### Scenario: It does not fit
- **WHEN** the CV needs 2 pages and 1 was chosen
- **THEN** approving is refused with "Doesn't fit on 1 page: switch to 2 pages or remove a few lines" and nothing is approved

#### Scenario: The PDF loses something
- **WHEN** the read-back cannot find a heading or a line
- **THEN** nothing is approved and the missing items are listed

### Requirement: Versions are kept and never overwritten
Every PDF made SHALL be kept as a version of its opportunity with its template,
length, pages, size, read-back results and dates. Choosing another template or
length on an approved CV SHALL make a new version from the same approved text
with no AI call, and earlier versions SHALL stay as they were. Another language
SHALL be a new draft.

#### Scenario: Compact
- **WHEN** someone picks Compact on an approved CV and confirms
- **THEN** version 2 exists beside version 1, whose PDF is unchanged, and no model was called

### Requirement: A PDF can be seen and downloaded by its owner only
`GET /api/v1/opportunities/{id}/cv/versions/{n}/pdf` SHALL serve the PDF inline
as `application/pdf` named "<Name> CV - <Company>.pdf" (ASCII fallback and
`filename*`), never cached or sniffed; `?download=1` SHALL serve it as an
attachment and record the download. Another account's version SHALL be not
found. `GET /api/v1/cvs` SHALL list the account's versions and live drafts,
newest first.

#### Scenario: Someone else's PDF
- **WHEN** another account asks for the PDF
- **THEN** the answer is 404 and no download is recorded

### Requirement: The app shows the PDF and lists every CV
The app SHALL open a PDF in a viewer (full screen on a phone, a large dialog on
a desktop) that draws every page, offers Download and closes with Escape, with
PDF.js loaded only when a PDF is first opened and from the app's own files. The
review page SHALL offer Open the PDF, the read-back results, template and
length controls (a change on an approved CV asks first and says no AI is used)
and the versions. The peek SHALL offer View PDF and Download. A CVs page SHALL
list every version and draft with All, Approved and Drafts filters.

#### Scenario: The CVs page
- **WHEN** someone has v1 and v2 of one CV and a draft for another job
- **THEN** /cvs lists all three, newest first, and View PDF opens a version
