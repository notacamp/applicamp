# Complete PDF and CV imports

Issue: https://github.com/notacamp/applicamp/issues/23

## Why
Importing a real LinkedIn PDF produced 13 positions with descriptions and
nothing else: no headline, summary, skills, education or languages, and the
dropped count was 0, so the source check was not at fault. The model was asked
for the whole profile in one call; the long positions took the answer and the
other sections never came back. Nothing checked why the model stopped, so a
cut-off answer was accepted as complete.

## What changes
- The text is read by two focused calls: the positions, and everything else
  (headline, summary, education, skills, languages, certifications, projects).
  Each names its sections and asks for all of them; the prompt explains that a
  two-column PDF puts its sidebar (Top Skills, Languages, Certifications)
  before or between the main sections.
- An answer the model stopped for length, or that is not the expected JSON, is
  asked again once with a bigger budget; if it fails again the import fails
  (`ai_failed`) and nothing is saved. A draft is complete or does not exist.
- The review lists the parts that came back empty ("No skills found in this
  file") with a hint: add them by hand, or import the LinkedIn data export.
- The source check is unchanged.

## Cost
Two calls instead of one: the document is sent twice, the answers are the same
size. Budgets stay per call (16k and 12k tokens, thinking included) and the
spend limits reserve each call's worst case as before.

## Modules touched
`profile` (server: imports/structure; client: import review).

## Non-goals
- Reading scanned PDFs, a different extractor, or layout-aware text extraction.
