# Design

- **Data** (schema `sources`, migration `sources__job_store`): `store_jobs` references its registry row
  (`board_id`, cascade) and is unique on `(board_id, source_ref)`; `content_hash` (a fingerprint of
  every stored field) tells a changed job from one seen again; `tags` is the site's registry tags as
  lower-case words, copied so the generated `search` tsvector (`simple` configuration, title weighted
  above company above tags) needs no join; `url_key` and `dedupe_key` are `dedupe::job_url` and
  `dedupe::dedupe_key`, not unique (the same job on two sites is two rows, merged by the search as
  today). `store_reads` keeps one line per read; the latest line of a site is its schedule
  (`next_read_at`) and holds its validators. Indexes: open jobs by `posted_at`, `last_seen_at`,
  `first_seen_at`, `closed_at`, `expires_at`, the two keys, the GIN on `search`; reads by
  `(board_id, at DESC)`.
- **One tick, not one job per site**: the server runs one worker, so a job per site would queue behind
  searches. `sources.read_store` runs every minute (it schedules the next first), selects up to
  `STORE_READS_PER_MINUTE` due rows in SQL (approved; a company page needs its system approved and
  free of the `robots_disallowed` blocker; `enabled` honoured; never-read first, job boards before
  company pages) and starts no read after 45 seconds. Rows are grouped by registry host; groups run
  concurrently up to `STORE_PARALLEL`, rows of a group one after the other, and the reader's own
  `wait_turn` keeps the pace between two requests to a host.
- **Due means all of**: its latest `next_read_at` has passed; `last_fetch_at + poll_minutes` has
  passed (`last_fetch_at` is also written by searches that read live, so the store and a search never
  both exceed the board's polling limit); `backoff_until` has passed. The interval written after a
  read is `max(poll, STORE_MIN_MINUTES, 24h / max_fetches_per_day)`, a day at least for a company
  page, then doubled as the site stays quiet: 1x for 0 to 2 quiet reads, 2x for 3 to 5, 4x for 6 to 9,
  8x for 10 to 19, 16x after, at most a day (a week for a company page), never below the base. A
  failure waits twice the base (a day at most), and a 429 or 503 waits at least its `Retry-After`
  (default an hour, a day at most), also written to the board's `backoff_until`.
- **Reading** reuses the readers: `Reader::store_read` is `read_board` with no cache and no skill
  queries (job boards, HN) or the system's reader through `board` (company pages). A `Probe` carries
  the stored `ETag` / `Last-Modified` as `If-None-Match` / `If-Modified-Since` on the first request
  (the first page of a paged API, the feed, the sitemap, a company page's list), notes the status and
  validators, and says when the read was **partial** (the page limit reached with more pages
  announced; every read through a sitemap). A 304 is `FetchError::NotModified`: no parse, every open
  job is touched, the validators stay. Searches never pass a probe, so their behaviour is as before.
- **Keeping** (`features/store/keep.rs`, one transaction): the read's jobs are deduplicated by
  reference; ones whose `expires_at` has passed (only kept for a site whose terms say
  `drop_expired`) are never stored and deleted if present; beyond `STORE_MAX_PER_SITE` the newest are
  kept; new or changed rows are upserted (reopening a closed job), unchanged ones touched. A read that
  is **full** closes what it did not list, except when that would close more than half of at least ten
  open jobs (taken for a partial answer, recorded as not closing). Cleaning, once an hour in the tick:
  expired jobs, jobs closed for `STORE_CLOSED_DAYS` (14), jobs no read has seen for 30 days (a site
  that stopped answering), the oldest above `STORE_MAX_TOTAL`, and reads older than 3 days except each
  site's latest.
- **Reading API**: `features::store::find(db, StoreQuery, now)`: open jobs, not expired, of approved
  sites (a company page's system approved too); posted since (first seen for a job that says none),
  first stored since (for "new since the last nightly search"), work modes (a job that says none
  passes), regions (case-insensitive; a job naming none passes), words (terms are alternatives, the
  words of one term all needed; letters and digits only), limit. `StoredJob::into_job` gives the
  `Job` the readers give, so scoring needs no second shape. It is not wired into searches.
- **API**: `GET /api/v1/boards/store`, session required, board owners only (403 `not_board_owner`, as
  the owner endpoints around it), `StoreTotals`: `on`, `jobs_stored`, `jobs_closed`, `new_7d`,
  `sites_read_today`, `reads_today`, `failures_today`, `sites_failing`, `last_read_at`.
- **No model, no run kind, no sandbox**: the reading is a server job over public APIs and feeds
  already approved. Terms and robots rules are the ones that approved each row; a sitemap site is read
  only on paths its stored rules allow, as for a search.
- **Fakes**: the e2e stack needs no change: its approved fake board is read at start and the
  Playwright test `store.e2e.ts` waits for the owner's totals to count its jobs (no screen).
- **Next part**: searches read `find` instead of the web, the company-pages setting
  `SEARCH_COMPANY_PAGES` goes away, and the Scanned view counts per board from the store.
