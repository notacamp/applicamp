# The import reads every section, and never puts a place in a summary

Issue: https://github.com/notacamp/applicamp/issues/72 (part 1 of 2; part 2 is
`complete-your-profile-facts`)
Module: profile (`features/imports`, `schema.rs`), plus the client's profile module (the import
panel and its review) and the fake model of the e2e stack.

## Why
A profile imported from a LinkedIn PDF can come back with the **place of each position as its
summary** ("Coimbra, Coimbra, Portugal", "Antuérpia e Região"), with no location and no end dates,
even though the PDF lists them. It can also have no highlights, no education, no languages, no
headline and no About text. The structuring prompt asks for `location` and `summary` separately.
But a small model can fill `summary` with the place line under the dates, and nothing caught it.
A CV drafter then sees only places and a few skills. The earlier fix (`complete-pdf-import`)
split the call in two, but the second call could still come back with empty sections.

## What changes
- **The positions' prompt is explicit**: `location` is the city, region or country line;
  `summary` is the paragraph that describes the work and is never a place; `highlights` are the
  bullet lines; `startDate` and `endDate` come from the dates line. `endDate` is left out when the
  position is current or the document gives no end date. The prompt's examples are invented.
- **The draft is put right in code, with no model, and with caution.** A position's `summary` that
  is a place moves to `location`, or goes when the position has one already. So does a place on
  the first line of a longer summary. A line counts as a place only in these cases:
  - it has commas, every part reads like a name, and it ends with a country ("Washington,
    District of Columbia, United States", "Porto, Portugal (Remote)");
  - it is the position's own location, or a part of it;
  - it is a region ("Lisboa e região", "Greater Example Area"), a country, or a place with a way of
    working ("Porto - Remote").
  The length, sentence and line-break checks come first. "Head of Engineering", "Customer
  Success", "Rust, Kafka, PostgreSQL" and "Sold to retailers in Spain, Portugal, France" are never
  places.
- **Other repairs, also in code**: a summary that only says the title or the company again goes,
  and bullet lines in a summary become highlights. Dates written as words or numbers become
  `YYYY-MM` (or `YYYY-MM-DD`): "Jul 2023", "julho de 2023", "set 2021", "07/2023", "15/03/2021",
  "15 Jul 2023". An end date that says "Present", "Present (1 year 4 months)", "o momento" or
  "aujourd’hui" goes. The source check still runs after this, so nothing is invented: a value only
  moves or goes.
- **An empty section is asked again.** The second call can return no education, skills,
  languages, certifications or projects although the text has that section's heading
  ("Education", "Languages", "Skills", "Competências", "Idiomas", "Formation"… in the app's four
  languages). It can also return no headline or no About text although the text has an About
  heading. In that case those sections are asked once more, in one call, on the text under those
  headings only. For the headline and the About text, that is the top of the document down to the
  end of the About text. The text sent is at most 8,000 characters across the six sections. If
  that call fails, the sections stay empty and the review says so. If a spending limit or the
  daily cap refuses it, the two answers already paid for are kept, and the review says that the
  limit stopped the second read.
- **The review says what was read**, in one plain line: "Found 6 jobs, 18 achievements, 9 skills,
  2 degrees and 2 languages." Below it are the parts that came back empty, now including "No
  achievements found" when no job has any.
- **Merge repairs a position read again, and never loses what someone wrote.** Positions are
  matched one to one: the company and the start first, and the title only when no position has
  that start. One position is never matched twice, so two stints with the same title at one
  company stay two. For a position matched this way:
  - its summary is touched only when the new read has a real paragraph. An empty summary takes it.
    A summary that is only this position's place gives way to it, and the place moves to an empty
    location. A summary is never replaced when the location holds a different place;
  - an empty end date, location or start is filled;
  - highlights are added once each.
  Everything the person wrote or kept stays.
- **A PDF picked as the LinkedIn export is read as a PDF** (and a ZIP picked as a PDF is read as
  the export), so the person does not have to switch the kind first.
- The copy of the import and its review is in plain words, in four languages. Merge says what it
  does: "Merge keeps what you edited, fills in what is missing and moves a place out of a job's
  summary."

## What the app does without the person, and what still waits on them
The app now repairs the draft (places, titles, bullets, dates) and asks again for empty sections by
itself. Merge repairs positions an earlier import got wrong. **Still waits on the person**:
choosing Replace, Merge or Discard, since nothing reaches the profile before that. They also need to
upload the file again once: uploads are never stored, so a profile imported before this change is
repaired only when the person imports the file again and merges it.

## Cost
When a section came back empty although its heading is in the text, the import makes one more
call, plus its one retry after a cut-off: at most two more calls. They send only the text under
those headings, at most 8,000 characters, with the same 12k/24k answer budgets as the second
call. They go through the account and instance spending limits like the other two. A refusal
there keeps the draft.

## Non-goals
- Repairing profiles already saved without a new import (uploads are not stored; the person
  re-uploads once).
- OCR, a different PDF extractor, or layout-aware extraction.
- Proposing facts from what the app already knows: part 2 (`complete-your-profile-facts`).
