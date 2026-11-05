# Design

## Data (schema `sources`, migration `…_sources__search_funnel.sql`)
- `sources.scores.stage` (`quick` | `full`, default `full` for every row from before) joins the
  primary key, so one job can have a quick look and a full read for the same versions. A quick
  row keeps the fit in `match_score` and its one reason in `why`; its facts are null.
- `sources.searches` gains `quick_looked`, `read_in_full`, `kept_with_gap` and the jsonb
  `dropped_by_reason`; null on a search from before (the Searches tab then shows `scored` as read
  in full and no list of reasons).
- `sources.board_scans.quick_looked` per site.
- No event is added or consumed. The pipeline is reached through the existing `Neighbours` port
  (two new methods, below).

## A search (`features/search`)
1. Read the sites (with the queries below) and the watched companies' boards; merge.
2. Jobs already in the pipeline are credited and go no further ("seen before").
3. Hard limits on the structured fields (`matching::hard_limit`), unchanged.
4. The cache: full reads and quick looks for this profile, goals and rules version.
5. The jobs without a full read, ranked by the old pre-rank (title against the wanted roles,
   freshness, place, skills) so a stop keeps the most promising. A watched company's new posting
   is read in full ahead of the cut, as before. When the rest fit in
   `FULL_SCORES_PER_SEARCH` (default 100), they are all read in full and no quick look is made.
   Otherwise every one without a quick look gets one (`quick.rs`): 25 a call, temperature 0, a
   JSON schema (`{"jobs":[{"ref","fit","reason"}]}`), `max_tokens` 5,000, the same retry in halves
   as full reads, the same stop rules. The best by quick fit (ties by the pre-rank) are read in
   full; the others are left for a later search, where their cached quick look competes again.
6. Full reads as today (`scoring.rs`, 10 a call), then the requirement rules, the bar (65) and
   the 25-a-search cap.

Time: one ten-minute budget for both stages; quick looks stop at half of it so the full reads
always get their turn. The job's lease stays 15 minutes.

### The funnel
Every job read lands in exactly one bucket, so the counts add up to `fetched`:
`seen_before` (in the pipeline already, a quick look or full read from an earlier search that is
not read now, or a match the pipeline already holds), `too_old` (older than 30 days or closed),
`not_remote` (the structured fields say it is not remote or not where they can work, or the full
read says it is not remote), `quick_look` (looked at now, not among the best), `language`,
`not_open_to_location` (the site's regions or the posting limit it to other places),
`below_floor`, `not_good_enough` (read in full, under the bar or with no reason the profile
supports), `waiting` (stopped by a limit, a failed call or the 25-a-search cap: the next search
takes them), and the good matches (`added`). `kept_with_gap` counts the jobs read in full now that
meet every requirement but carry the "doesn't say if it hires in" note.

## Rules (`requirements.rs`, `RULES_VERSION` 3)
For someone who works remote only, or would not move: `remote = yes` and `open_to_candidate =
unclear` passes with the gap "Doesn't say if it hires in <country>" (the country of where they
live, its English name from the place table; "Doesn't say where it hires" when their country is
unknown). `open_to_candidate = no` still drops (`not_open_to_location`); `remote` not yes still
drops for remote only. The language and pay rules are unchanged. The gap is stored in English
like every other gap; the client shows this one and "Salary not stated" in the person's language,
the country named by the browser (`Intl.DisplayNames`).

### What is already in the pipeline
`sources.recheck_requirements` (at every start) also pages through Found and Shortlisted
opportunities hidden for `not_open_to_location` whose posting said `open_to_candidate = unclear`,
and applies the rules again to the facts stored on them (and the pay last read for that job, when a
cached score has it). One that now passes is shown again with the gap added; one that fails
another rule stays hidden. No model call. Ports: `Neighbours::hidden_unclear(after, most)` and
`Neighbours::show_with_gap(account, opportunity, gaps)`, implemented by the pipeline store
(`hidden_unclear`, `unhide_with_gaps`), only while still Found or Shortlisted and hidden for that
reason.

## Queries (`matching::board_queries`, `readers`)
The server's match input gains `queries`: for each wanted role (at most 3), its words without
seniority or level words (`senior`, `staff`, `lead`, `principal`, `junior`, `mid`, `head`, `I`,
`II`, …) and without the parts after a comma or a bracket; then the top 3 skills; without repeats
(ignoring case), at most 6. A site with a skill search (`skill_search.param`) is asked each, one
page, as the board spells it (`SkillSearch::value_of`), with each address cached for the board's
polling interval stretched to its daily limit (Jobicy: 7 addresses at 24 a day is one fetch of
each every 7 hours). The nightly reading reads the first 6 of everyone's queries once.

`json_api` keeps the pages it read when a later page fails (an error or an answer that is not the
JSON expected): it logs the board and the error kind and the site's line says "Read 2 pages. Page
3 did not answer." A failed first page still fails the site.

## API (scope: a signed-in person, `session`)
- `GET /api/v1/boards/scans`: each search run gains `funnel` (`read`, `quick_looked`,
  `read_in_full`, `good_matches`, `kept_with_note`, `left_out` with the buckets above, `stopped`);
  each line gains `quick_looked`. Discovery runs have no funnel.
- `GET /api/v1/searches/estimate`: the jobs a search would look at and read in full, from the last
  search; the cost from the average cost of a call, about. A search asks before starting only above
  €0.25 (was €0.05): the line under the button says the cost.

## AI
Both stages use the `complete` role. Without a model the search says `ai_unavailable` as before.
Nothing a stage does needs undoing: a quick look only ranks, and the person still decides on every
match.

## Client (Searches tab, `ScannedView`)
Per run, four numbers; the latest run open with its two lists; earlier runs one row each with
Details; "Show by site" for the table (site, jobs read, quick look, read in full, good matches,
cost, and a failed or partial read in words); a discovery run is one row ("Looked for new job
sites") with its checks under Details. "The app searches every night at 21:00.", Search now (no
shortcut hint) and "About €0.10." from the estimate. Buttons 44 px tall on phones, 36 px on
desktop (the design system's sizes). Four languages.
