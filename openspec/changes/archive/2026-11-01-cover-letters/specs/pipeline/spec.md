## ADDED Requirements

### Requirement: A cover letter needs an approved CV and one model call
For an opportunity whose CV is approved, a click SHALL write a cover letter with exactly one model call that is
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

## MODIFIED Requirements

### Requirement: The app shows the PDF and lists every CV
The app SHALL open a PDF in a viewer (full screen on a phone, a large dialog on
a desktop) that draws every page, offers Download and closes with Escape, with
PDF.js loaded only when a PDF is first opened and from the app's own files. The
review page SHALL offer Open the PDF, the read-back results, template and
length controls (a change on an approved CV asks first and says no AI is used)
and the versions. The peek SHALL offer View PDF and Download, and a Cover letter
line (draft or approved, with Review and View PDF). A CVs page SHALL list every
version and draft of CVs and of cover letters with All, CVs, Letters and Drafts
filters.

#### Scenario: The CVs page
- **WHEN** someone has v1 and v2 of one CV, a cover letter v1 and a draft for another job
- **THEN** /cvs lists all four, newest first, View PDF opens a version, and Letters shows only the letter
