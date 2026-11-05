# Design

## The pool (`features::store::pool`)
One SQL query returns the open, unexpired jobs of approved sites: the person's On sites
(`b.id = ANY(sites)`) and every company page whose hiring system is approved, without their
descriptions (they are up to 10,000 characters), plus a `rank` column: `ts_rank(search, query, 32)`
(rescaled to 0..1) plus `0.3 / (1 + age_days / 7)`. The query is the person's terms or-ed, each term's
words and-ed (`store::text_query`): each wanted role's key words (`matching::role_words`, the roles they
had when they named none, at most 5) and the words of the top skills. No filter on the terms: a job
that matches nothing ranks by age alone, so a person with no roles still gets the newest. The
`first_seen_since` filter is the nightly's "new since the last search" (the person's last finished
search, strictly after). The pool holds 100,000 jobs at most.

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
