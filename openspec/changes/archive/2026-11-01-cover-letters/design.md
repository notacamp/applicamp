# Design: the cover letter

## Why a sibling of the CV, not `kind: cv | letter` on the CV's tables
The brief preferred generalising the CV pipeline. What is generalised: the sources (`Frame`: the profile cut into
ids plus the company facts), the claim check (`applicamp_kernel::claims` and `Frame::evidence`), the model call
(`brief::ask`, the budget, the retry), the company facts and wants, the neighbours port (`cv_profile`,
`keep_fact`), the Typst world, the bounds and the read-back (`pdf::make_prepared`, `readback::check`,
`PdfChecks`), the PDF headers, the draft lifetime and its sweep, and the client's review components. What is not:
a CV's content is positions with bullets and skills, a letter's is paragraphs of sentences and answers, and every
CV table, endpoint and view is shaped by the first. A `kind` column would make `content` a union read by every
CV path and every CV test, and would put the letter's caps (10 and 20, "counted separately from CV drafts") in
a `WHERE kind =`. So the letter is `features/cv/letter/`, a child module of the CV (it uses its private parts),
with `letter_drafts` and `letter_versions`, and the lists that show both (`GET /api/v1/cvs`) carry
`document: cv | letter`. The CV's own code changed only to share: `pdf::make_prepared`, `prompt::blocks`, a few
visibilities, and the expiry sweep, which also deletes expired letter drafts.

## Data (the pipeline schema owns it)
`pipeline.letter_drafts` (account, opportunity, n, language, size, cv_version, status, version, content, frame,
checks, dates, expiry) and `pipeline.letter_versions` (the PDF, size, pages, read-back, dates), both
`ON DELETE CASCADE` from the opportunity. No events are published or consumed. Drafts expire seven days after
their last change; approved versions are kept.

## Endpoints (scope: the signed-in session, own account only; another account's is 404)
`GET /api/v1/opportunities/{id}/letter`; `POST …/letter/drafts`; `PATCH …/letter/drafts/{n}/sentences/{id}`;
`POST …/sentences/{id}/confirm`; `PATCH …/letter/drafts/{n}/answers/{id}`; `POST …/letter/drafts/{n}/approve`;
`POST …/letter/drafts/{n}/discard`; `GET …/letter/versions/{v}/pdf`.

## The check
A sentence is `{text, sources}`. With no valid source it passes only if it claims nothing (a closing, an offer
to talk: "No claims"); a claim with no source is flagged. With sources, every claim must be in their text
(plus the head of their positions, and the company's name for a company fact). If all valid sources are company
facts and the sentence is about the person (a first-person word in en/pt/es/fr, unless it says what draws them:
admire, like, want…), it is flagged. Superlatives come from a small fixed list; a cited source that says the word
clears it. An answer needs text and sources, or it is flagged empty; one the person wrote is theirs and is not
checked. All reasons are data (`LetterReason`), worded by the client in four languages. Nothing is dropped.
Known limit: a sentence with no sources and no claim words that nonetheless says something about the person
("I led the team") passes, because the check has no model and no content heuristics; the prompt and review cover
it, and the person sees every sentence's sources.

## The prompt and privacy
`You write a cover letter…` (`PREFIX`, version 1): temperature 0, `complete` role, one call, with the output
schema `{paragraphs: [{text, sentences: [{text, sources}]}], answers: [{question, text, sources}]}`. The message
holds the language, the length in words, the posting, the wants, the company facts, the profile sources, the
approved CV's headline, summary, bullets and skills as plain lines, and the numbered questions. No name, email,
address or link is in the frame or the CV lines; a test asserts it. Everything shown is data (angle brackets made
plain). The greeting, sign-off, name and contact are printed by the server.

## The PDF
`letter.typ` shares the Clean template's page, font, name and contact line. The letter is JSON in `sys.inputs.cv`,
shown as text only. The read-back expects the greeting, every paragraph, the sign-off, the name, the email and the
links, on one page; a letter that does not fit one page is not approved (`letter_doesnt_fit`). The answers are
never given to the template.

## Who may do what, and undo
The person clicks every model call (draft, Make again). Approve is one transaction: PDF first, then the approval
and the version. A discarded or approved draft cannot change; "Make again" starts the next draft. AI role:
`complete`; without a configured model the button says why and nothing is attempted.

## Client
`LetterPage` at `/pipeline/$id/letter` built from the CV review's pieces (`Paper`, `CvLine`, `ReviewLayout`,
`RowList`, `PdfViewer`). `LetterSlot` ("Write the cover letter · about €0.002") is used on the letter page, the CV
review page and, as Review/View PDF, the peek's Cover letter line. The CVs page filters by `document` and `kind`.
