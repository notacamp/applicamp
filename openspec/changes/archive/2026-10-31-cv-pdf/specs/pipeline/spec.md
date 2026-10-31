## ADDED Requirements

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
