# Design

## Board registry (`sources.job_boards`)
`id, name, homepage, kind (json_api|rss|jsonld_sitemap|hn), priority (high|low), status (candidate|approved|paused|declined), config jsonb, terms jsonb, check jsonb, approved_at, approved_by, recheck_after, created_at`.
- `config`: endpoint(s), pagination (cursor field / page param, max pages), **field mapping** (JSON pointer or key per normalized field: `title, company, url, apply_url, location_text, regions[], remote, salary_min/max/currency/period, posted_at, expires_at, description, tags, language`), poll interval (min), delay (Remotive 24 h is already applied by them), required query params, user-agent.
- `terms`: attribution text, link-back required (bool, "follow"), apply link unchanged (bool), no redistribution (bool), drop expired (bool), max fetches/day, notes + source URLs of the clauses.
- Seeded (as `approved`, owner decision 2026-10-27) from `crates/modules/sources/job_boards.toml`: **Himalayas** (`/jobs/api`, cursor, `locationRestrictions`/`timezoneRestrictions` → regions, salary fields; ≤ 60/min, hourly), **Jobicy** (`/api/v2/remote-jobs`, `jobGeo` → regions; ≤ 1/hour), **Remotive** (`/api/remote-jobs?category=software-dev`; ≤ 4/day), **RemoteOK** (`/api`; skip the first legal element; `Crawl-delay: 1`), **Arbeitnow** (`remote=true` only; hourly), **HN Who is hiring** (existing extractor). `declined` rows with reasons: Landing.jobs (robots `/api/` disallowed + ToS forbids scraping), The Muse (US-centric, stale), Wellfound/Dice (robots). `candidate` rows needing the owner: We Work Remotely (ToS unread: human must read), Working Nomads (ask permission). Owner 2026-10-27: "i dont care for job boards in portugal … it should look for remote jobs anywhere … they dont have to be dropped but they're not prio". Every board gets a `priority` (`high` = remote-first with worldwide reach, `low` = national/regional or general boards). The seeded approved boards are all `high`. National/regional boards (ITJobs.pt — free key by email, Net-Empregos RSS, Adzuna — free key, personal use only, SwissDevJobs…) are `low`-priority `candidate` rows: kept, listed after the high-priority ones on the Boards page, never blocking anything.

## Ranking boards by average salary (owner 2026-10-27: "they should be ranked by average salary")
- Every board keeps `stats`: `avg_salary_eur_year` = mean of the midpoints of stated salary ranges (a single value counts as itself), converted to EUR per year with the same fixed table as the salary-floor rule, over its **relevant** jobs (engineering roles open to the person's country) from the last 30 days; `salaried_jobs` (n), `relevant_jobs`, `salary_share` = salaried / relevant. Recomputed after each fetch (approved boards) and at the discovery check (candidates, from the sample).
- **The Boards page and the discovery approval queue are sorted by `avg_salary_eur_year` descending.** Each row shows "avg €118k/yr · from 64 of 210 jobs (30% state a salary)". Boards with fewer than 10 salaried jobs show the average greyed with "few salaries"; boards with no salary data sort last with "no salary data". Priority (high/low) is shown as a label, it does not override the order.
- **Country-scoped boards** (owner 2026-10-27: "if they're scoped to a country, the most important should be the ones in countries that pay more"): a board whose jobs are mostly in one country gets `country` (from its config or the check). Its ranking key is its own `avg_salary_eur_year` when it has ≥ 10 salaried relevant jobs; otherwise the **country benchmark**: a reference table `crates/modules/sources/country_pay.toml` of typical senior software-engineer pay per country (EUR/yr, with the source and date of each figure, from a public salary survey; refreshed yearly), shown as "≈ €95k/yr (Switzerland benchmark)". Worldwide remote boards always rank on their own average. So a Swiss or US board ranks above a Portuguese one even when they list few salaries.
- The same per-board figures are available to search for later use (e.g. which boards to fetch first when limits are tight); search results themselves are still ordered by match.

## Adapters (one per kind)
- `json_api`: GET with config, paginate, map fields by JSON pointer, HTML → text. One test fixture per seeded board (recorded, trimmed, invented text).
- `rss`: Atom/RSS 2.0, encodings (ISO-8859-1), mapping incl. title splitting ("Company: Role") by a configured separator; region from a configured element.
- `jsonld_sitemap` (stub + tests, no seeded board yet): sitemap → job URLs (≤ N per run) → `application/ld+json` `JobPosting` → fields; only for boards approved with robots allowing those paths.
- Shared: 10 s timeout, 4 MB cap, per-host rate limit from config + `Retry-After` back-off, cache honouring the poll interval, user agent `applicamp/<version> (+https://applicamp.not-a-camp.com)`.

## Search changes
- Sources = approved job boards only (company boards off; flag `APPLICAMP_SOURCES__COMPANY_BOARDS=false` default).
- Hard requirements (#19) unchanged; the new structured `regions` (Himalayas/Jobicy) feed the pre-filter: a job whose structured regions exclude the person's country (e.g. "USA only", "Germany") is dropped before the model; jobs open worldwide or including the person's country/continent pass. The search itself has no regional preference: remote jobs anywhere, as long as they are open to where the person lives.
- Dedupe: canonical apply URL (scheme+host+path, tracking params stripped: utm_*, ref, source, gh_src…), then `lower(company)+lower(title)+location` within 30 days. `pipeline.opportunity_sources(opportunity_id, board_id, url, seen_at)` keeps every source.
- Expiry: boards whose terms require it → hide opportunities past `expires_at` (Found/Shortlisted only).

## Discovery (weekly job `sources.discover_boards`)
1. Candidates: the CC0 list `emredurukn/awesome-job-boards` README (names + URLs only), RSS `<link rel=alternate>` and `/api` / `/jobs/api` probes on approved boards' partner pages, and URLs the owner adds ("Suggest a board").
2. Check each new domain once (≤ 20 per run, 1 request per path, robots respected): robots.txt (job/API/feed paths allowed?), terms page found? + sentences matching scrap|robot|automat|aggregat|API|link back|attribution|commercial (stored verbatim, max 10), feed/API/JSON-LD found, sample ≤ 20 jobs parsed with a proposed mapping, share of remote jobs, share open worldwide or to the person's country (from region/location fields), language (English).
3. Result → `candidate` with `check` filled and a `priority` from the check (high when most jobs are remote and open worldwide; low otherwise); the queue is sorted by average salary (see Ranking); never fetched again until approved. Re-check approved boards' robots/terms every 6 months (flag changes for the owner).
No model calls in discovery by default; an optional model pass to propose a field mapping for JSON APIs counts against the spend limits.

## Client: Boards page
List by status (Approved, Waiting for you, Declined). A candidate shows the check: endpoint, robots verdict, the terms sentences, sample jobs, proposed attribution — **Approve** / **Decline (with reason)** / **Edit config** (JSON editor for the mapping, validated). Approved boards show last fetch, jobs read, errors, Pause. "Suggest a board" (URL). Pipeline peek shows "From Himalayas ↗" (followed link) for each source.

## Scanned view (owner 2026-10-27: "a view of job boards scanned by AI")
- `sources.board_scans(id, search_id null, discovery_run_id null, board_id, at, outcome (read|skipped_poll_limit|error|checked), jobs_read, passed_requirements, scored_by_ai, matched, ai_cost_eur, error_kind, duration_ms)`: one row per board per search (and per discovery check). Kept 90 days.
- The **Boards** page gets a second tab, **Scanned**: newest first, grouped by search ("Today 21:00 · nightly search · €0.012"), one line per board: jobs read → passed your requirements → scored by AI → matched, cost, or "skipped: polling limit" / the error; discovery checks listed with what the check found. Filter by board. Totals for the last 7 / 30 days per board (read, matched, cost) on each board's row in the main tab.
- Same numbers available in `GET /api/v1/boards/scans?board=&search=&cursor=`.

## Cost
Fetching is free; discovery uses no model by default. Search cost unchanged (model only after filters, top 40).

## Implementation notes (where the build differs from, or adds to, the plan above)
- **Registry.** `sources.job_boards` as planned, with the check in a column named `report`
  (`check` is an SQL keyword), plus `host` (one board per site, unique), `origin` (seed,
  discovery, suggested), `config_edited`, the last fetch (`last_fetch_at`, `last_fetch_jobs`,
  `last_error`) and `backoff_until`. The seed list (`job_boards.toml`, or
  `APPLICAMP_SOURCES__JOB_BOARDS_FILE`) is synced at start: new boards added with their status;
  known ones get the seed's name, site, kind, priority, country and terms, and its config unless
  the owner edited it; a status is never reset. Typed schemas are `BoardKind`, `BoardStatus`,
  `BoardPriority`, `BoardOrigin`, `BoardTerms`, `BoardReport`.
- **Remotive is seeded as a candidate**, not approved: its robots.txt disallows `/api/*` and
  `/*category=` for every crawler (read 2026-10-27), while its API terms invite this use. The
  approval rule below blocks it; `openspec/decisions-needed.md` asks the owner. The other
  approved boards' endpoints are allowed by their robots.txt.
- **Adapters.** `json_api`: JSON pointers per field (`fields`), `items`, cursor or page-number
  pagination (`cursor {param, from}`, or `page_param` + `next`), `skip_first` (Remote OK's legal
  notice), an item `filter` (Arbeitnow's own `remote: true`), defaults for a board's pay
  currency and period. `rss`: a small reader for flat RSS 2.0 and Atom (CDATA, entities, Atom
  `link href`, ISO-8859-1 and windows-1252 as Latin-1), element names as the mapping, a title
  split by separator. `jsonld_sitemap`: a sitemap's `<loc>`s under `job_path` (at most
  `max_jobs`, 50), each page's `JobPosting`s mapped with schema.org's fields, only on paths the
  board's checked robots rules allow (gzipped sitemaps are not read; no seeded board uses it).
  Times are seconds or milliseconds since 1970, RFC 3339, RFC 2822, or a date-time without a
  zone (UTC). HN keeps its extractor as the `hn` kind. The old Arbeitnow reader is gone:
  Arbeitnow is a `json_api` board; its jobs keep the source `arbeitnow` and their refs.
- **Fetching.** A second client for registry boards reaches public hosts only: its DNS resolver
  drops private, loopback, link-local, CGNAT, documentation, multicast and reserved addresses,
  an IP written in the address is checked the same way, and only `https` on its default port
  (`APPLICAMP_SOURCES__ALLOW_PRIVATE_HOSTS=true` lifts this for tests and a laptop demo). 4 MB
  cap, 10 s timeout, no redirects, `min_interval_ms` per board on top of the instance's pace.
  The polling interval is kept through the cache: each answer is reused until the interval is
  over, and such a read is `skipped_poll_limit` in the scans (a nightly search counts the
  nightly job's own read, 15 minutes before it, as read). A 429 or 503 backs the board off for
  its `Retry-After` (an hour when absent, a day at most). User agent `applicamp/<version>
  (+https://applicamp.not-a-camp.com)`.
- **Search.** Approved boards only; `APPLICAMP_SOURCES__ENABLED` now names board slugs (and
  company board kinds). Company boards are off unless `APPLICAMP_SOURCES__COMPANY_BOARDS=true`.
  The regions pre-filter drops a job only for someone who works remote only or would not move,
  only when they live in a country the place table knows, and only when every region the board
  gives is understood (countries, Europe/EU/EMEA, worldwide words, other continents) and none
  includes them; anything else goes to the model. Expired jobs (boards whose terms drop them)
  are a hard limit, and Found/Shortlisted opportunities past `expires_at` leave the list and
  Today's count.
- **Dedupe.** The canonical link is the apply link (else the posting's) on https, the host
  without `www.`, the path without a trailing `/`, tracking parameters stripped and the rest
  sorted. The fallback key is `company|role|place` with the place as the sorted countries the
  location and regions name (none for "anywhere" and for unstated, so they meet). A job already
  in the pipeline under another board is credited there (`pipeline.opportunity_sources`), by link
  first, else by key within 30 days. The pipeline keeps `apply_url`, `expires_at`,
  `canonical_url`, `dedupe_key`; its `source` is any board slug (the `CHECK` became a pattern);
  the API's `OpportunitySource` enum became a string with `source_name`, and an opportunity has
  `sources` (board, name, link, first seen) and `apply_url`.
- **Ranking.** Computed when the Boards page asks, for the viewer's country (from where they
  live), from `sources.board_jobs`: what each board listed in the last 30 days (title, regions,
  engineering by its title's words, salary midpoint in euros a year), refreshed at every fetch
  and every check's sample. Nothing is stored as `stats`: search can read the same function
  later. `country_pay.toml` holds Eurostat's Structure of Earnings Survey 2022 (mean annual
  earnings of professionals, ISCO-08 group 2, enterprises with 10+ employees) for European
  countries and the Stack Overflow Developer Survey 2025 (median back-end developer pay,
  converted at 0.92 EUR per USD) for the United States, the United Kingdom and India; every
  other country is left out (its boards rank on their own salaries, or last).
- **Discovery.** Weekly on Mondays 04:00 UTC (`APPLICAMP_SOURCES__DISCOVERY`, on by default),
  at most 20 checks a run: new sites from the CC0 list (its section gives a first priority, and a
  country for a national section), then candidates never checked that have an endpoint
  (suggested, or seeded). A check reads each path once: robots.txt of the feed's host (and of
  the site's when it is another), the homepage (its `lang`, feed links, JSON-LD postings, the
  link to its terms), the terms page (up to 10 matching sentences, verbatim), then the first
  readable of the config's endpoint, the homepage's feeds on the same host, `/api` and
  `/jobs/api`, whose answer gives up to 20 sample jobs through a proposed config (the proposal
  replaces the kind and config of discovered and suggested boards, never a seeded or edited
  one). High priority when at least half the sample is remote and half open worldwide. Not
  built: reading approved boards' partner pages for candidates, and the optional model pass that
  proposes a mapping (off by default in the plan).
- **Re-check.** Approved boards whose `recheck_after` is due (seeded ones at the first run):
  robots.txt and the terms page are compared with the last check's fingerprints; a change, or a
  feed now disallowed, is a flag for the owner; the board is never paused by itself; next one in
  182 days.
- **Owner.** The board owners are the addresses in `APPLICAMP_SOURCES__BOARD_OWNERS` (the
  server gives the signed-in account's address from identity). They approve, decline (a reason,
  1 to 500 characters), pause, edit the config (validated; unknown keys refused; an approved
  board's new feed must stay allowed) and re-run a check; anyone else gets 403
  `not_board_owner`. Approval needs a config that reads the board's kind and a check of the
  feed's host whose robots rules allow the feed's path (409 `config_invalid`, `check_needed`,
  `robots_disallowed`). Anyone signed in sees the boards and suggests one (a public https site,
  one board per site, 10 a day; its check runs at once as `sources.check_board`).
- **Scanned.** `sources.board_scans` per board per search (scoped to the search's person) and
  per discovery check (everyone's); kept 90 days. Read = jobs the board listed; passed = within
  the hard limits on the structured fields; scored = scored by the AI now or before; matched =
  kept as Found (at most 25). Each batch's AI cost is split between its jobs, then between the
  boards a job was seen on. `GET /api/v1/boards/scans` pages runs ten at a time by time.
- **Endpoints** (session): `GET /api/v1/boards`, `GET /api/v1/boards/{id}`,
  `POST /api/v1/boards/{id}/approve|decline|pause|check`, `PUT /api/v1/boards/{id}/config`,
  `POST /api/v1/boards/suggestions`, `GET /api/v1/boards/scans`.
- **Client.** "Boards" in the sidebar under Pipeline (G then B), with the tabs Boards and
  Scanned (`/boards?tab=scanned`); the sections Approved (and paused), Waiting for you, Declined,
  each by average salary; a peek with the check and, for owners, Approve, Pause, Check again,
  Decline with a reason and a JSON config editor. The Pipeline peek credits each board ("From
  Himalayas", followed: `noopener noreferrer`, never `nofollow`) and offers "Apply" with the
  board's apply link unchanged.
