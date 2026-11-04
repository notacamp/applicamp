# The import reads every section, and never puts a place in a summary

Issue: https://github.com/notacamp/applicamp/issues/72 (part 1 of 2; part 2 is
`complete-your-profile-facts`)
Module: profile (`features/imports`, `schema.rs`), plus the client's profile module (the import
panel and its review) and the fake model of the e2e stack.

## Why
On the hosted instance, a profile imported from a LinkedIn PDF came back with every position's
**place as its summary** ("Lisboa, Lisboa, Portugal", "Bruxelas e Região"), no location, no end
date on any position although the PDF lists "Jul 2023 – Apr 2024", no highlights, no education, no
languages, no headline and no About text: 13 positions and 4 skills. The structuring prompt asks
for `location` and `summary` separately, but the model (GLM-5.3 Flash) filled `summary` with the
place line under the dates, and nothing caught it. A CV drafter then sees only places and four
skills. The earlier fix (`complete-pdf-import`) split the call in two; the second call still came
back with empty sections.

## What changes
- **The positions' prompt is explicit**: `location` is the city, region or country line;
  `summary` is the paragraph that describes the work and is never a place; `highlights` are the
  bullet lines; `startDate` and `endDate` come from the dates line ("Jul 2023 - Apr 2024"), and
  `endDate` is left out only for "Present".
- **The draft is put right in code, with no model**: a position's `summary` that is a place (the
  position's own location, or a line of at most 40 characters that reads like "City, Region,
  Country" and says no verb) moves to `location` (or goes, when the position has one already); a
  summary that only says the title or the company again goes; bullet lines in a summary become
  highlights; dates written as words or numbers ("Jul 2023", "julho de 2023", "07/2023") become
  `YYYY-MM`, and an end date that says "Present" goes. The source check still runs after this, so
  nothing is invented: a value only moves or goes.
- **An empty section is asked again**: when the second call returns no About text, education,
  skills, languages, certifications or projects although the text has that section's heading
  ("Education", "Languages", "Skills", "Competências", "Idiomas", "Formation"… in the app's four
  languages), the sections are asked once more, in one call, on the text under those headings only.
  If that fails too the sections stay empty and the review says so; a spending limit stops the
  import as before.
- **The review says what was read**, in one plain line: "Found 13 jobs, 31 achievements, 14 skills,
  1 degree and 3 languages.", above the parts that came back empty (as today).
- **Merge repairs a position read again** instead of leaving it as it was: a summary that is only a
  place (an earlier import's mistake, not an edit the person made) moves to the location and the
  new paragraph takes its place; an empty end date, location or summary is filled; highlights are
  added once each. Everything the person wrote or kept (an edited summary, kept facts) stays.
- **A PDF picked as the LinkedIn export is read as a PDF** (and a ZIP picked as a PDF as the export):
  the person does not have to switch the kind first.
- Copy of the import and its review in plain words, four languages.

## What the app does without the person, and what still waits on them
The app now repairs the draft (places, titles, bullets, dates) and asks again for empty sections by
itself, and Merge repairs positions an earlier import got wrong. **Still waits on the person**:
choosing Replace, Merge or Discard (nothing reaches the profile before), and uploading the file
again once (uploads are never stored, so a profile imported before this change is repaired only
when the person imports the file again and merges it).

## Cost
At most one more call per import, only when a section came back empty although its heading is in
the text, on the text under those headings only (a few hundred tokens in; the same 12k/24k answer
budgets as the second call). It goes through the account and instance spending limits like the
other two.

## Non-goals
- Repairing profiles already saved without a new import (uploads are not stored; the person
  re-uploads once).
- OCR, a different PDF extractor, or layout-aware extraction.
- Proposing facts from what the app already knows: part 2 (`complete-your-profile-facts`).
