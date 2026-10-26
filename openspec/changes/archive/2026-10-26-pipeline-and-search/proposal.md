# Pipeline and a first job search

Issue: https://github.com/notacamp/applicamp/issues/5

## Why
After setting up a profile there is nothing to do: no jobs, no pipeline
(owner, 2026-10-26: "after i setup my profile what there to do? i cant run
agents myself or pipelines"). The owner chose to get a usable pipeline and a
first search now, ahead of sandboxed agent runs.

## What changes
- **Pipeline** (`pipeline` module): every opportunity in one Notion-style
  database with a table view and a board by stage, a side peek with its
  properties, why it matches and its gaps, the posting link, notes, stage
  changes and "Not for me". Add any job yourself by pasting its link.
- **First search** (`sources` module): read jobs from public job-board APIs
  (Greenhouse, Lever, Ashby, Arbeitnow, HN "Who is hiring"), keep the ones
  that fit your goals' hard limits, score the rest against your profile and
  goals with the `complete` model, and put the good matches in the pipeline
  as **Found**. "Search now" on Pipeline and Today, plus a nightly search.
- **Today** reflects where you are: profile and goals done or not, new
  matches since your last visit, and the next step.

## Modules touched
`pipeline` (new), `sources` (new). Client: `modules/pipeline`, Today.

## Non-goals
- Running searches in sandboxes (they move into agent runs when `agent-runs`
  lands; the search code is written so it can run inside the agent runner).
- Applying, CVs, email.
- LinkedIn or Indeed in any form (ADR 0006).
