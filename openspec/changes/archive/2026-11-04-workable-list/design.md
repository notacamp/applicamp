# Design

- **Streaming** (`features/workable_list/feed.rs`): `Scanner::push(chunk)` appends to a small
  buffer, finds each `</job>`, parses the block before it and drains it. Fields: `company`,
  `referencenumber`, `url`, `date` (RFC 2822 with `UTC`), and `website` taken after
  `</description>` (a description may hold anything). A block over 4 MiB without its end is
  dropped. Tested with the same fixture cut into chunks of 1 to 1000 bytes.
- **Download** (`workable_list::download`): `reqwest` `Response::chunk()` loop; the client has the
  total timeout and a 60 s read timeout, no redirects, the public-only resolver unless private
  hosts are allowed. Counts per company are kept in a map flushed (one `unnest` upsert) every
  20 000 jobs and at the end, however it ends. Within a run counts add up; a new run replaces
  them (`run` column).
- **State** (`sources.workable_list_state`, one row): validators of the last complete download,
  `not_before` (a 429), last outcome and counts. A run is skipped when the last started less
  than 20 hours ago.
- **Token lookup**: `GET <WORKABLE_URL>/j/<code>` with no redirects; the `Location`'s first path
  segment is the token. Failures wait 7 days (1 day if unreachable); a 429 or 503 ends the day's
  lookups.
- **Registry**: `INSERT ... SELECT FROM unnest ... ON CONFLICT (system, lower(company_token), eu)
  DO UPDATE` on the #87 unique index; a page already found by an apply link only gains
  `list_seen_at` and `list_jobs` (new columns on `sources.job_boards`). `approve_listed` runs
  after every read and at the start of the daily company-page run; pages with `list_seen_at` are
  left out of the sample-read queue. A list sighting older than 14 days no longer proves hiring.
- **Fakes**: `workable.hiring.test` serves `/boards/workable.xml` (invented companies Lantern
  Ferries and Quillfeather Studio with recent jobs, Dust Ledger Press with a 2021 job) and the
  short-link redirect; `APPLICAMP_SOURCES__WORKABLE_LIST_URL` points the stack at it.
