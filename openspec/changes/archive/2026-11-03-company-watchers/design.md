# Design: company watchers

## Data (the sources schema owns it)
`sources.companies` (id, key, name, website, careers_url, board_kind, board_token, board_eu, detected_at, checked_at, origin) describes
a company and its job board and is shared: the key is `site:<host>` (or `ats:<kind>:<token>` for a company only known by its board), so
two people who watch the same site share the detection. `board_kind` is `greenhouse`, `lever`, `ashby`, `jsonld` or `none`; `origin` is
`matches`, `pasted` or (later) `agent`. `sources.company_watches` is one person's relation to a company: `state` (`suggested`,
`watching`, `not_interested`, `stopped`), the traced `reasons` (jsonb), when it was started, when a board is being looked for
(`finding_since`, dead after 10 minutes), the last read (`last_checked_at`, `last_error`, `open_jobs`) and when its postings were noted
as seen (`seeded_at`). `sources.watch_seen` holds the posting keys already seen per watch, pruned 60 days after a posting left the
board. `sources.watch_refreshes` says when a person's suggestions were last worked out. `sources.board_scans` can name a company
instead of a job board (`board_id` is nullable, `company_id` added, exactly one set), so a watch check is a scan row.

The matches come from the pipeline through the `Neighbours` port (`matched_postings`, `company_facts`); the server wires them. No module
imports another, `cargo xtask arch` passes. The pipeline learns that an opportunity came from a watched board by its credited source
`company-board` (`opportunity_sources.board`), added from the `Scored.own_board` the search sets.

Events: none new. `sources.search_finished` and `pipeline.opportunity_added` are unchanged.

## Why the daily check runs inside the search
The brief allowed a separate job or part of the nightly. A separate `sources.watch_check` job would score in a batch of its own, count
its own budget and scan row, and need its own recurrence (the trap of `enqueue_unique_at` counting the running job: only the nightly's
`enqueue_next_at` is right). The nightly job already fans out one search per person; a watch is read by that person's search, which
already reads job boards, hard-limits, pre-ranks, caches scores, batches the model calls within the spending limits, records scans and
adds matches. So the watched boards' new postings join the search's candidates (past the pre-rank's cut of 40, up to 20 a board), and
`record_scans` writes one row per watched board next to the job boards'. "Search now" does the same, so the check can be tried without
waiting for the night, and a board is read at most once per `watch_every` (20 hours) whichever search comes first. The nightly job
starts a search for a person with a due watch even when no job board has anything new. The nightly job still reschedules itself with
`enqueue_next_at`; `FindBoard` is enqueued by a click, never scheduled.

A posting is noted as seen only when the search is done with it: scored (whatever the score), left out by a hard limit, or already in
the pipeline. One the model could not score (a spending limit, a failed call) stays unseen and is scored by the next search.
The first read after a board is found only notes what is there; the watch is "from today".

## Endpoints (scope: a signed-in session, each person's own rows; 404 for anyone else's)
`GET /api/v1/companies` (works out stale suggestions), `POST /api/v1/companies` (a pasted careers page or board link),
`POST /api/v1/companies/{id}/watch`, `/not-interested`, `/stop`, `/careers`. Each answers the whole list. 409 `watch_limit_reached`
past 20, 400 `invalid_url`. `GET /api/v1/today` gains `company_hits`.

## Detection and reading
Pages go through `Reader::page` (public addresses only, same-site redirects, a size cap, a pace) after the site's robots.txt (`pages::Rules`)
allows them; a page robots.txt forbids is never requested. A board's API is asked through `Reader::board` at the configured bases with a
checked token; a pasted link to a Greenhouse, Lever or Ashby board is never fetched, its token is read from the link. Ashby, Lever and
Greenhouse tokens are found in links, embeds and API addresses by fixed patterns; JSON-LD is read with the existing `jsonld` reader. LinkedIn,
Indeed, Glassdoor and hiring platforms are never a company's site.

## AI
No new role. Scoring uses the `complete` role through the search's existing path (`scoring::score`), so the budget, the daily cap and the
cache apply unchanged. Without a model the search stops (`ai_unavailable`) and nothing is read; suggesting and finding a board need none.
The person can undo: Stop watching (the board is not read any more) and, for a mistaken Not interested, pasting the careers page again.

## Agent work
None. No run kind, no sandbox. A later run kind that finds companies adds rows to `sources.companies` through a typed tool; a watch hit
that needs an agent can bridge to a standing order (ADR 0009); neither is needed now.
