# A job store: every approved site is read on its own schedule

Issue: https://github.com/notacamp/applicamp/issues/89 (epic #85, ADR 0010), first of two pull
requests. This one builds the store and its reading; **searches are unchanged** and still read
boards live. The second one (after #74) makes searches read the store, company pages included.

Modules: `sources` (the registry's readers, a new `features/store` slice, one owner endpoint).

## Why
A search reads every approved board live while it runs. With the registry holding thousands of
company pages that is too slow and too many requests, and a board's terms (polling interval, fetches
a day) are hit by whoever searches first. Reading each site once, on its own schedule, and letting
searches read what was kept, keeps every site within its terms and a search fast.

## What changes
- **A job store** (`sources.store_jobs`): the jobs the readers already produce, normalised (title,
  company, links, place, work mode, pay, dates, description cut at a limit, the site's tags, the
  dedupe keys the search already uses), one row per site and job, with when it was first and last
  seen and when it closed. A text search (title, company, tags), posted and last-seen indexes.
- **A log of reads** (`sources.store_reads`): one line per read, with its status, counts, the HTTP
  status, the site's `ETag` / `Last-Modified` and when to read it next.
- **A reading every minute** (`sources.read_store`): takes the approved registry rows that are due
  and reads them through the readers that exist, within each site's own limits, a host one request
  at a time and a global number of reads a minute. Only new or changed jobs are written; jobs a full
  read no longer lists are closed and deleted after 14 days; expired jobs are dropped; a quiet site
  is read less often; a company page is read once a day at most. Every read's apply links go to the
  company-page registration, as a search's do.
- **A read function for the second part**, inside the module (`features::store::find`): open jobs of
  approved sites, filtered on posted since, work mode, regions and words, with a limit. No endpoint.
- **`GET /api/v1/boards/store`**, for board owners: jobs stored, new in 7 days, sites read today,
  failures. For the admin page (#86) to show later.
- **Settings**: `APPLICAMP_SOURCES__STORE` (on) and the limits in `config.rs`.
- **Fakes**: the e2e stack's approved fake board fills the store at start.

## What the app does by itself / what waits on the person
By itself: reads every approved site and company page on its schedule, keeps new and changed jobs,
closes and deletes what the site dropped, slows down for quiet sites and for sites that ask. Waiting
on the person: nothing new; approving a site is still what makes it readable, pausing or declining it
still stops it, and the settings above are the limits.

## Cost and limits
No model call. Requests to a site never exceed its polling interval, its fetches a day or its
`Retry-After`; an hour apart by default, a day for company pages, further apart when it stays quiet.
At most 60 reads a minute over all hosts, 8 hosts at once, one request at a time on each. Jobs kept:
5000 per site and 500000 in all (settings); descriptions cut at 10000 characters.

## Non-goals
Searches reading the store (the second part, #89), the Scanned view counting from the store, the
admin page showing the totals (#86), reading sites live when someone searches, detecting a job's
language.
