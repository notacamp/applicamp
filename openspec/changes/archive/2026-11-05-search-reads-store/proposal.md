# Searches read the job store, with company pages, through a cheap preselect

Issue: https://github.com/notacamp/applicamp/issues/89 (second part; epic #85, ADR 0010)

Module: `sources` (search, the store's read side, readers, the Searches tab in
`clients/app/src/modules/sources`).

## Why
The first part of #89 reads every approved site and company page into `sources.store_jobs` on its
own schedule (41,000+ open jobs in prod). A search still read every approved board live, ignored
the thousands of company pages, and would have sent every job past the hard limits to the quick
look, which at tens of thousands of jobs would use up the per-account cap of EUR 1 a day in one night.

## What changes
- **The candidates come from the store.** A search draws its jobs from the job store: the open jobs
  of the person's On job sites (every approved site until each person chooses theirs, #94), every
  approved company page (which covers the companies the person watches), and, for a nightly search,
  only the jobs the store first saw since that person's last search. Live board reads in a search
  stop, except the person's own queries (roles plus skills on sites with a keyword search), which
  stay live because the store reads only default feeds; their jobs are merged in and deduplicated
  as before. With the store off (`APPLICAMP_SOURCES__STORE=false`) a search reads the sites live,
  as before the store.
- **A cheap preselect, no model.** After the hard limits and the cache, the jobs with no look yet are
  ranked with Postgres full text (`ts_rank` of the store's `search` vector, the title, company and
  the site's tags, against the words of the person's wanted roles and top skills) plus how new they
  are, and only the top `APPLICAMP_SOURCES__QUICK_LOOK_PER_SEARCH` (400) go to the quick look. The
  others are counted as "Not among the closest to your roles today" and stay for a later search.
  A job already looked at keeps competing for a full read at no cost; a job read in full takes no
  slot, as before.
- **Cost.** The search records the cost it expects (quick-look calls plus full-read calls at the
  person's average price per call) before its first call, and `GET /api/v1/searches/estimate` counts
  the jobs of the store without a score, at most the quick-look limit, so "ask above EUR 0.25"
  sees the new candidate counts. Spending limits and stop rules are unchanged.
- **`APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES` is removed.** Company pages are always candidates,
  through the preselect. The setting was never set in prod.
- **Searches tab.** The funnel gains the reason "Not among the closest to your roles today" so the
  counts still add up, and a line "Read from N sites and M company pages". Each On site keeps its
  line, with what the store gave plus what its live queries gave.

## What the app does by itself / what waits on the person
By itself: reads all candidates from the store, picks the closest ones with no model, looks at them,
reads the best in full, adds the good matches, and says what it did and what it cost. Nothing new
waits on the person: the limits are configuration.

## Cost
A store of 40,000 jobs and a nightly with 1,500 new jobs: the preselect keeps 400, which is 16 quick
looks (25 a call) plus 10 full reads (100 in 10 a call), 26 calls, about USD 0.03 (EUR 0.03) at
GLM-5.3 Flash prices; without the preselect the 1,500 would be 60 quick looks, and a first manual
search over 40,000 jobs 1,600. A manual search takes the closest 400 of the whole store, the next
search the next 400.

## Non-goals
- Per-person On/Off sites (#94, in review; until it lands every approved site is On).
- Reading company pages of watched companies differently: the daily watch check still reads their own
  board to spot new postings.
- Embeddings or any model in the preselect.
