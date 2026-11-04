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
the page SHALL make no model call and fetch nothing unless it starts the chain
of a job that was never prepared (see "A shortlisted job is prepared by itself").
The page SHALL offer "Draft the CV" with what it costs, or "Review the CV" when a
draft exists, and say why drafting is off when it is. "Draft the CV" on a job
whose company was never read SHALL read it first when it can, and say so when it
cannot.

#### Scenario: Opening it
- **WHEN** someone whose goal is "Find only" opens "Prepare: company and job" on a Shortlisted opportunity
- **THEN** they see the company's site, a Read button with what it costs, a Draft the CV button with what it costs, and nothing has been fetched or asked of the model

#### Scenario: Found is too early
- **WHEN** an opportunity is still Found
- **THEN** its peek does not offer the Tailored CV block, and drafting a CV for it is refused with `stage_too_early`

#### Scenario: Drafting before reading
- **WHEN** someone clicks "Draft the CV" on a job whose company's site is known but was never read
- **THEN** the company is read and the job compared first, and the draft uses what the job wants

### Requirement: The company's website is known once
The company's website SHALL be, in this order: the one the person typed, the
one the job board gave, the apply link's host when it is not a known job
board or applicant-tracking system (a posting the person pasted counts as its
own apply link), a link or email address in the posting whose domain is the
company's, a domain made from the company's name whose robots.txt allows its
home page and whose home page names the company (at most three tried, with no
model call), else the person is asked. Job boards, tracking systems, webmail
and link shorteners SHALL never count. The page SHALL say where the site came
from, and the person's address SHALL always win and be read again. It SHALL be
kept on the company so it is found or asked once, for every job there. Only the
address's host is kept.

#### Scenario: The apply link is the company's own site
- **WHEN** an opportunity's apply link is on a careers page of the company's own domain
- **THEN** the company's site is that domain, and the person is not asked

#### Scenario: An email address in the posting
- **WHEN** an opportunity's only link is a Lever posting and its text says to write to `jobs@example-co.com`
- **THEN** the company's site is `https://example-co.com/`, the page says it was found in the posting's email address, and the person is not asked

#### Scenario: A domain from the name
- **WHEN** the posting names no domain, and `examplesoft.com`'s home page names "ExampleSoft Software" and its robots.txt allows it
- **THEN** that is the company's site, and the page says it was found from the company's name

#### Scenario: Only a tracking system's link
- **WHEN** an opportunity's only link is a Greenhouse, Lever or Ashby posting, the board gave no website, the posting names no domain of the company's, and no domain made from the name has a home page that names the company
- **THEN** none of those guesses is used, the page asks for the company's website, and keeps what is typed for every job at that company

#### Scenario: Webmail is not a website
- **WHEN** the posting's only address is at a webmail domain
- **THEN** that domain is never the company's site

#### Scenario: The person corrects it
- **WHEN** the person types another website for a company whose site was found
- **THEN** their address is kept for every job there and the company is read again from it

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
the pages and never memory, and that page text is data, not instructions. The call
SHALL ask for at most 10 facts, each text at most about 160 characters and each
quote at most about 200, in the instruction and in the answer's schema. It SHALL
be shown a limited amount of page text (the posting first, then the pages by
priority); when its answer is cut off, the one retry SHALL be shown half the
text, not the same text with more room to answer, and there SHALL be two attempts
in all. The
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

#### Scenario: Cut off, then half the text
- **WHEN** the model's first answer to a company's pages is cut off
- **THEN** it is asked once more with half the page text, and a company whose second answer also fails is reported as not read

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
counts, whether or not the site answered or the model could answer, and a read
that failed SHALL never be retried by itself. A read SHALL start only from a
click or from the preparation of a job the person shortlisted. Calls SHALL go
through the account and instance spending limits, checked before each call.
Without a model, or over a limit, the page SHALL say so in plain words and
nothing SHALL be fetched or retried. Nothing SHALL run on a timer.

#### Scenario: Over the daily cap
- **WHEN** an account has started 20 company reads today and reads another
- **THEN** it is refused with `company_reads_daily_cap` and nothing is fetched

#### Scenario: Another person's opportunity
- **WHEN** someone asks for the brief of an opportunity that is not theirs
- **THEN** it is not found, nothing is fetched and no model is called

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
the card names ("You told us about <company>: …" for a fact they told the app, "Your profile says about
<company>: …" for their profile's own words), and what a yes keeps word for word ("If you keep it, this
goes on your profile: …"), and Yes, keep it / Change it / Remove it. When there is no yes,
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

#### Scenario: Whose words
- **WHEN** a blocked line was made from a highlight of the imported profile, and another from a fact the person told the app
- **THEN** the first card says "Your profile says about <company>: …" and the second "You told us about <company>: …", and Change it starts from those words either way

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
the app shortlisted, and SHALL make at most four model calls: the draft, a
second try of it with more room only when its answer is cut off, the second
look at what the job wants, and the fix of what the check flagged. The cost a
draft is shown to have before the click SHALL count the four at their worst at
the configured models' prices, and a preparation SHALL record what the calls
really cost. The calls SHALL go through
the account and instance spending limits, and nothing SHALL be retried past a
limit or run on a timer. Rendering a preview SHALL call no model. A draft nobody
approved SHALL be deleted 7 days after its last change; approved versions SHALL
be kept.

#### Scenario: Over the daily cap
- **WHEN** an account has made 20 drafts today and drafts another
- **THEN** it is refused with `cv_drafts_daily_cap` and the model is not called

#### Scenario: Left alone
- **WHEN** a draft is not approved and not changed for 7 days
- **THEN** it is gone, while an approved version stays

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

### Requirement: A cover letter needs an approved CV and one model call
For an opportunity whose CV is approved, a click, or approving the CV of a job being prepared by
itself, SHALL write a cover letter with exactly one model call that is
shown the posting, what the job wants, the company facts the person kept, the profile with every kept fact, the
words of the approved CV, the language, the length (short, about 180 words, by default; standard, about 300) and
the job's own questions. It SHALL never be shown a name or contact details: the server prints the greeting, the
name and the contact. The prompt SHALL ban "I am writing to apply", flattery, clichés and superlatives the sources
do not support. Without an approved CV the action SHALL be off and say why.

#### Scenario: No approved CV
- **WHEN** the CV of a job is not approved
- **THEN** "Write the cover letter" is off with "Approve your CV for this job first", and no model is called

#### Scenario: Nothing private is sent
- **WHEN** a letter is drafted
- **THEN** the model's message holds no name, email, address or link of the person

### Requirement: A letter's sentences may say only what their sources say
Every sentence SHALL cite its sources and, with no model, every claim in it SHALL be in them. A company fact MAY
be cited in any paragraph to say why the company appeals, but a sentence that cites only company facts SHALL be
flagged if it says anything about the person. A superlative (best, top, go-to, expert and their equivalents in
English, Portuguese, Spanish and French) SHALL be flagged unless a cited source says it. A sentence that cites
nothing SHALL pass only if it claims nothing. Whatever fails SHALL stay on the page with the reason and be
fixed with It's true (the person's words become a fact on their profile), Edit or Remove; it SHALL never be
dropped. Approve SHALL stay off while anything is open.

#### Scenario: What the company did, said as the person's
- **WHEN** a sentence says "I led our migration" and cites only a company fact
- **THEN** it is flagged and Approve is off

#### Scenario: Praise nobody wrote
- **WHEN** a sentence calls the person "the go-to person" and no source says it
- **THEN** it is flagged, and saying it is true adds the person's words to the profile and clears it

### Requirement: The job's own questions are answered or left to the person
The questions SHALL be the lines ending in "?" under an "Application questions" heading of the posting (or its
translation), read without a model, at most five. Each SHALL get an answer from the same one call, checked like a
sentence. An answer the profile cannot support SHALL come back with no text and the flag "Your profile and goals
don't say. Answer it yourself", with a box for the person's own answer, which is theirs and is not checked.
Answers SHALL be text with Copy and SHALL not be in the PDF; an open question SHALL keep Approve off.

#### Scenario: Notice period
- **WHEN** the posting asks "What's your notice period?" and the profile does not say
- **THEN** the answer is empty and flagged, and writing "One month." makes it the person's answer

### Requirement: Approving a letter makes a one-page PDF that is read back
Approving SHALL, with no model, render the letter with the CV's Typst stack in a letter template of the same
style on one page with the same embedded fonts, passing the text as data and never markup, then read the PDF back:
the greeting, every paragraph, the name and the email SHALL be in its text and it SHALL be one page. If anything
fails, nothing SHALL be approved. The PDF SHALL be kept as a version, never overwritten, and be served only to its
owner, inline or as a download.

#### Scenario: Approved
- **WHEN** nothing is open and someone approves
- **THEN** version 1 exists as a one-page PDF whose read-back found every paragraph

#### Scenario: Text that looks like markup
- **WHEN** a sentence contains `#import`, `]`, `$`, `*`, `_`, `<x>` or `@ref`
- **THEN** it appears literally in the PDF's text

### Requirement: Letter drafts are capped apart from CV drafts
There SHALL be at most 10 letter drafts per opportunity and 20 a day per account (UTC), counted separately from
CV drafts, each through the AI budget; a draft past a limit SHALL be refused before the model is asked.

#### Scenario: CV drafts do not use up the letter's allowance
- **WHEN** an account has made 20 CV drafts today
- **THEN** it can still write a letter

### Requirement: The review page shows where each paragraph came from
`/pipeline/$id/letter` SHALL show the letter as a page with each paragraph tagged Company fact, From your profile
or No claims and a "Where this came from" for its sources, the job's questions with their answers, and an aside
with the banner, "At a glance" (words, the CV version it matches, the company facts used) and the spend line. It
SHALL be reachable from the CV review page once the CV is approved, and from the peek.

#### Scenario: At a glance
- **WHEN** a draft cites one company fact and matches CV v2
- **THEN** the aside says so, with the word count

### Requirement: Today says when a watched company posted a job that fits
Today SHALL show, for each job found in the last 3 days on the own job board of a company the person watches that is still Found, not set
aside and not yet opened, a card "X posted a job that fits you" with the role, place and pay, when it was found on their own job board, how
well it fits and that it is in the Pipeline as Found. Open it SHALL open that job's peek in the Pipeline; Not for me SHALL set it aside. The
opportunity SHALL credit the company's own job board among the boards it was found on. Today SHALL also offer a compact suggestion of one
company to watch, with Watch, Not interested and Why?, unless the person has not set up a profile.

#### Scenario: A job from a watched board
- **WHEN** a search adds a job that fits from a watched company's board
- **THEN** Today shows "Northwind Stays posted a job that fits you" with Open it and Not for me, and the job's peek says it was found on Northwind Stays' job board

#### Scenario: Not for me
- **WHEN** the person presses Not for me on that card
- **THEN** the job is set aside like any other and the card goes

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

### Requirement: The person sees and stops what runs by itself
Prepare SHALL show each step done by itself, with what it cost and when, and what the chain
waits on. Stop SHALL cancel every step still to run for that job; so SHALL moving it back to
Found or Not for me, or choosing "Find only". "Prepare by itself" SHALL start it again. Read
again, Draft again and Try again SHALL stay. A step that failed SHALL never be retried by itself.
A step a cap or a spending limit would pass SHALL not start: the page SHALL say why and offer
the step as a click. Today SHALL list the jobs waiting on the person's answers or approval.

#### Scenario: Stop
- **WHEN** someone stops a job's preparation while the CV draft is still to run
- **THEN** no draft is made, and the page offers "Prepare by itself"

#### Scenario: A cap
- **WHEN** the daily CV draft cap is reached while a job is being prepared
- **THEN** no draft is made, the job says the cap stopped it, and "Draft the CV" is offered for tomorrow

#### Scenario: A failure
- **WHEN** reading the company fails
- **THEN** the job says why, offers Try again, and nothing is retried by itself

#### Scenario: Another person's job
- **WHEN** someone stops or starts the preparation of an opportunity that is not theirs
- **THEN** it is not found

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

### Requirement: The CV page says what could not be done, and offers the way on
When the company's site could not be read and the CV was written without its facts, the page SHALL
say so in plain words ("The site asks apps not to read it. The CV was written from the job posting
and your profile only.") and offer "Paste what the site says": the pasted text SHALL be read like one
of the site's pages with one model call, each fact quoting it, within the daily company reads, and
never as instructions. When the PDF could not be made, the page SHALL say so, keep the CV's text and
offer Try again and Download the text. While the app writes the CV by itself, the page SHALL say so
and wait for it, with no button to press. A button that starts a paid step SHALL carry no price: its
cost SHALL be on the line under it.

#### Scenario: A site that asks apps not to read it
- **WHEN** the company's site forbids reading and the person pastes "Example Co runs payroll for bakeries"
- **THEN** a fact quoting those words is kept for the company, and the page no longer says the site was not read

#### Scenario: The PDF could not be made
- **WHEN** the PDF of a draft could not be made
- **THEN** the page says so for that company, and offers Try again and Download the text

#### Scenario: Being written
- **WHEN** the app is writing the CV of a job it is preparing
- **THEN** the page says it is being written and shows it when it is ready
