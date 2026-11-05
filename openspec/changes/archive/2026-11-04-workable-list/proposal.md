# The Workable list: every Workable company that is hiring, once a day

Issue: https://github.com/notacamp/applicamp/issues/88 (epic #85, ADR 0010)

Modules: `sources` (the registry, the daily job `features/workable_list`, the fakes).

## Why
#87 made company hiring pages rows of the registry, found one apply link at a time. Workable
publishes one XML list of every job its customers publish, documented for programmatic use. One
download a day can grow the registry by thousands of Workable companies at once, with no model.

## What was verified (2026-11-04)
- The URL `https://www.workable.com/boards/workable.xml` is the one Workable's help article
  "Utilizing the XML Job Feed" names. It answers 200, `application/xml`, `Content-Length` about
  864 MB, `Last-Modified` (no `ETag` seen) and `Cache-Control: max-age=0, must-revalidate`; a
  `HEAD` answers with an empty body, so the job sends a conditional `GET`.
- Documentation quoted: "The XML feed contains all job information for all our customers
  published on Jobs by Workable. You will need to read/download the full file and then filter the
  data on your end to get the jobs for one company only." "The XML link should be consumed
  programmatically; due to the size, it may not be possible to load this via the browser." And its
  condition: "This is an advanced option and should not be used to connect with job boards that
  have existing integrations with Workable." Job links (`https://apply.workable.com/j/<code>`)
  are how a candidate reaches the job, so they are kept exactly as given.
- `www.workable.com/robots.txt`: `User-Agent: *`, `Content-Signal: search=yes, ai-input=yes,
  ai-train=no`, `Disallow` for `/user_password_resets`, `/admin`, `/auth/google` and `/j/`. The
  path of the list is allowed. `apply.workable.com/robots.txt`: `Disallow:` (nothing) and the same
  signal. The job checks robots.txt before every download.
- **The list does not carry the company's address on Workable.** Each job's link is
  `apply.workable.com/j/<code>`, never `apply.workable.com/<token>/...`; a request to it answers
  `301` to `/<token>/j/<code>`. The issue assumed the token was in the link; it is not.

## What changes
- **A daily job** (`APPLICAMP_SOURCES__WORKABLE_LIST`, on by default, 06:00 UTC, one download in
  20 hours at most, also once at the first start) streams the list: each chunk goes to a scanner
  that hands over a job's company, short code, date and site, and forgets the block. Memory holds
  one unfinished job and a batch of company counts, never the body.
- **Conditional requests**: `If-None-Match` and `If-Modified-Since` from the last complete
  download; a `304` ends the read. A `429` or `503` is honoured (`Retry-After`, an hour to a day).
- **Limits**: `..._MAX_BYTES` (1.5 GB) and `..._TIMEOUT_SECS` (an hour, a minute of silence);
  a download cut by either, or interrupted, keeps what it read: counts are written every 20 000
  jobs, and a cut download is never the one the next request is conditional on.
- **Only counts are kept** (`sources.workable_list`): company name, site, the short code of its
  newest job, how many jobs the list had and how many were posted in the last 60 days. The jobs
  themselves are not stored (a job store is #89). No model is called.
- **Finding the company's address**: for each company with a job in the last 60 days, one request
  to its newest job's short link gives the token (a redirect, never a page read), newest first, at
  most `..._LOOKUPS` (500) a day, robots rules of the apply host respected, one request a second.
  A link that already carries the token needs no request.
- **The registry grows**: each such company is upserted, in batches, as a Workable `company_page`
  ("the Workable list", with when the list last had it and how many jobs).
- **Approval follows #87's rule**: approved by the app when the Workable hiring system is
  approved. The list proves the company posts jobs, so the sample read is skipped and the row's
  report says so. While Workable waits for the owner (its `ai-train=no`), its pages wait with it;
  a declined or paused system is not read.
- **The exclusion list** (`..._EXCLUDE`, empty): names or sites of boards Workable already
  integrates with; a company on it is never added from the list. Workable publishes no such list.

## What the app does by itself / what waits on the person
By itself: the daily download and parse, the lookups, adding the company pages, approving them
once Workable is approved. Waiting on the person: Workable's own approval (its `ai-train=no` is
the owner's choice, as in #87), and the exclusion list's entries.

## Cost
No model. One download a day of about 860 MB (less when `304`), at most 500 small requests.

## Non-goals
Storing the jobs (#89); reading the company pages in searches; the admin page (#86).
