# Design

## The pool (`features::store::pool`)
One SQL query ranks and limits in the database, so the server never holds more than
`APPLICAMP_SOURCES__SEARCH_POOL` (3,000) jobs of a search. It covers the open, unexpired jobs of
approved sites: the person's On sites (`b.id = ANY(sites)`) and every company page whose hiring
system is approved. The cheap hard limits that need only columns apply in SQL (not older than 30
days, a way of working they want); region and remote-only limits stay in memory. Descriptions are
left out (up to 10,000 characters each). Rows come `ORDER BY` "has a score of this person's for this
profile, goals and rules" (unscored first, so new jobs are never crowded out), then `rank DESC`, then
id, `LIMIT` the pool size. `rank` is `ts_rank(search, query, 32)` (0 to 1) plus
`0.3 / (1 + age_days / 7)`; the query is the person's terms or-ed, each term's words and-ed
(`store::text_query`): each wanted role's key words (`matching::role_words`, the roles they had when
they named none, at most 5) and the words of the top skills. A job that matches nothing ranks by age.
`first_seen_since` is the nightly's "new since the last search".

A plain `count(*)` over the same filter (`pool_counts`) gives the jobs the pool left out: too old,
not a way of working they want, and past the pool's limit. A search counts them under `too_old`,
`not_remote` and `preselect` ("Not among the closest to your roles today") and adds them to the jobs
read, so the funnel still adds up.

## A search (`features/search`)
1. `read_sources`: the pool, plus the person's live queries (`Reader::job_board_queries`: one page
   per query on the sites with a `skill_search`, nothing else, leaving `last_fetch_at` alone so
   the store's schedule is untouched, backing a site off when it says slow down), merged and
   deduplicated as before. One scan line per On site: the store's jobs plus its live queries.
2. Pipeline credit, hard limits and the cache are as in #74, run on the light jobs.
3. **Preselect.** Jobs without a full read or a quick look that came from the store are sorted by
   rank (ties by key) and cut at `QUICK_LOOK_PER_SEARCH`; the cut is `left_out.preselect`. Jobs
   with a cached quick look, jobs from live queries and watched postings are never cut.
4. Descriptions of the chosen jobs are read from the store (`store::descriptions`) before the
   quick look, and of the jobs read in full or added before they are used or kept.
5. The quick look, full reads, requirement rules, the bar and the cap are unchanged.

The funnel still adds up: every job of the pool lands in exactly one reason or is a match.

## Nightly
`sources.nightly` warms the shared live queries once, then starts a search for a person when a job
the store first saw since their last finished search, or a live query job, passes their hard limits
and has neither a quick look nor a full read, or a watch is due. A night with new store jobs without a
score always runs; a night without them does not, however many older jobs still wait.

## Data
Migration `sources__search_reads_store`: `sources.searches.from_sites`, `from_company_pages`
(nullable) and `expected_cost_usd`.

## API (scope: a signed-in person, `session`)
- `GET /api/v1/boards/scans`: each search funnel gains `from_sites` and `from_company_pages` and
  `left_out.preselect`.
- `GET /api/v1/searches/estimate`: with the store, `jobs` is the unscored store jobs capped at the
  quick-look limit, and the cost covers those quick looks plus the full reads.

## Config
`APPLICAMP_SOURCES__QUICK_LOOK_PER_SEARCH` (400) added; `APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES` removed.
