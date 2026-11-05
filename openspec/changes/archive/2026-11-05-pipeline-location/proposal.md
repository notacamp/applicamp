# The pipeline shows where each job is and what it asks of where you are

Issue: https://github.com/notacamp/applicamp/issues/111
Modules: `sources` (the scoring call reads three more facts and the score cache keeps them),
`pipeline` (keeps them on the opportunity and sends them with every row), the server's wiring, and
the client's pipeline module (table, cards, board, peek). No new endpoint.

## Why
The owner (2026-11-05): "In the pipeline I want to see the location of the company and location
based requirements." Where a job is was shown only in the side peek and on phone cards, never in the
desktop table. What a posting asks of where the person is (only from some countries, a time zone, a
work permit, moving) was only partly read: the region it is open to, yes; the hours, the right to
work and visa sponsorship, and relocation, no. Those are often what decides whether a job is worth
opening.

## What changes
- **The same scoring call reads three more facts** from each posting, in its own words and never
  guessed, each at most 80 characters and left empty when the posting does not say: the working
  hours or time zones it needs ("CET ± 2 hours"), what it says about the right to work and visa
  sponsorship ("EU work permit, no visa sponsorship"), and what it says about moving ("must relocate
  to Berlin, relocation package"). No extra call, no extra cost beyond a few output tokens.
- **They are shown, never used to hide a job.** The requirement rules do not change, so the rules'
  version stays 3 and cached scores are not asked again (no full re-scoring night); jobs read from
  now on carry the notes.
- **Stored** on `sources.scores` (`place_notes`, jsonb) and on `pipeline.opportunities`
  (`time_zone_note`, `work_permit_note`, `relocation_note`). Two additive migrations.
- **API**: `JobFacts` gains `time_zone_note`, `work_permit_note` and `relocation_note`;
  `OpportunitySummary` (each row of the list) gains `facts`, so a row can say what it asks.
- **The pipeline**: the desktop table gets a **Where** column, the place and work mode ("Berlin,
  Germany · Hybrid") with, under it, what it asks of where you are: "Open to where you live: EU ·
  Hours: CET ± 2 hours · Work permit: EU work permit, no visa sponsorship · Moving: must relocate to
  Berlin". Phone cards and the board show the work mode next to the place and the same line. The
  peek adds Hours, Work permit and Moving under Your requirements when the posting says them. In
  the four languages.

## What the app does by itself, and what waits on the person
The notes are read by the search the app already runs; nothing new runs and nothing waits on the
person.

## Non-goals
- Where the company itself is based (its head office): the owner chose where the job is.
- Hiding or ranking jobs by time zone, work permit or relocation (a goal for that would come first).
- Reading the notes for opportunities already in the pipeline: a search never updates one it
  already has, and a backfill would spend on every account; a follow-up if wanted.
