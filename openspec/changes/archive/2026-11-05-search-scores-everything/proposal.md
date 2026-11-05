# Search looks at every job within your limits, and a job that does not say where it hires is kept with a note

Issue: https://github.com/notacamp/applicamp/issues/74 (ADR 0010)

Module: `sources` (search, readers, scans, the Searches tab of the Job sites page in
`clients/app/src/modules/sources`), with a port to `pipeline` for showing again what was hidden
only for "unclear", and `ai` for the default spending limits.

## Why
On the hosted instance a nightly search read about 800 to 1,300 jobs, kept about 350 to 950 within
the hard limits, and sent only 40 to the model, those 40 shared with jobs it had already scored and
rejected in the last 30 days. A remote job whose posting did not say it hires in the person's
country was dropped as "unclear": 65 of 172 scored jobs went that way, some scoring 85. Boards were
asked with the top 3 skills only, never with the roles the person wants; Himalayas read 5 pages of
its newest jobs of every kind; one failed page failed the whole board. Result: 0 to 5 jobs added a
night, and the Scanned view could not say why.

## What changes
- **Two stages.** Every job within the hard limits that has no score for this profile, goals and
  rules version gets a **quick look**: one `complete` call per 25 jobs with the title, company,
  place, way of working and the first 400 characters of the posting, answering a fit from 0 to 100
  and one short reason. The quick look is kept in `sources.scores` with `stage = 'quick'`. The best
  `APPLICAMP_SOURCES__FULL_SCORES_PER_SEARCH` (100) by quick fit are then **read in full** as
  today (10 a call, 1,500 characters, the posting's facts). A job read in full never takes a slot
  again; a rejected job is read again only when the profile, goals or rules version changes. When
  every job without a full read fits in the slots, no quick look is made (it would change nothing).
  The stop rules stay (four failed calls in a row, ten minutes), and every call reserves its budget
  first; at a spending limit the search keeps what it has and says `ai_budget_reached`.
- **"Unclear" is a note, not a drop.** For someone who works remote only, or who would not move, a
  remote job whose posting does not say whether it hires where they live is kept with the note
  "Doesn't say if it hires in Portugal" (their country). A posting that limits it to other places
  still drops. The language rule and the pay floor stay. `RULES_VERSION` 2 → 3, so cached scores
  are read again once; the requirements recheck at start shows again, with that note, the
  opportunities that were hidden only for this.
- **Sites are asked for the person's roles and stack**: up to 3 wanted roles without seniority
  words ("ruby developer", "software engineer") and the top 3 skills, without repeats, at most 6
  queries a site, one page each, on the sites that search (Himalayas `q`, Jobicy `tag`, Remote OK
  `tag`). Himalayas reads 10 pages. A page that fails keeps the pages already read and says so on
  the site's line.
- **The Searches tab shows the funnel.** Per search: jobs read, read in full, good matches, cost.
  Under the latest: where the matches came from, why the other jobs were left out (counts that add
  up to the jobs read), and "N of the M read in full don't say where they hire. They are kept, with
  a note." The table by site is behind "Show by site"; earlier searches are one row each with
  Details. "Search now" is a plain button with "About €0.10." on the line under it.
- **The nightly search** still skips a person when no new job within their limits is unseen; a job
  counts as seen once it has a quick look or a full read.
- **Spending limits** default to €1 a day and €10 a month per account (were €0.30 and €3); the
  instance stays at €15 a month.

## What the app does by itself / what waits on the person
By itself: looks at every job within the limits, reads the best 100 in full, keeps a job that does
not say where it hires with a note, asks the sites for the person's roles, shows what it did and
what it cost. Nothing new waits on the person: the limits are configuration and the spend shows on
the page.

## Cost
At GLM-5.3 Flash prices ($0.15 / $0.50 per million tokens): a quick look of 25 jobs is about
$0.001, a full read of 10 about $0.001 to $0.002. A first night with about 950 jobs within the
limits: about 38 quick looks and 10 full reads, about €0.04 to €0.06. Later nights look only at
the new jobs (a few hundred): about €0.02 to €0.03. Well under the new €1 a day.

## Non-goals
- New hard limits, or a change to the language and pay rules.
- Reading the HN thread differently (its comments go through the quick look like any job).
- The Sites tab, the board registry, discovery and the hiring-system readers (other changes).
- Translating the model's own reasons and gaps (they stay in English, as today).
