# Design

## Board registry (`sources.job_boards`)
`id, name, homepage, kind (json_api|rss|jsonld_sitemap|hn), priority (high|low), status (candidate|approved|paused|declined), config jsonb, terms jsonb, check jsonb, approved_at, approved_by, recheck_after, created_at`.
- `config`: endpoint(s), pagination (cursor field / page param, max pages), **field mapping** (JSON pointer or key per normalized field: `title, company, url, apply_url, location_text, regions[], remote, salary_min/max/currency/period, posted_at, expires_at, description, tags, language`), poll interval (min), delay (Remotive 24 h is already applied by them), required query params, user-agent.
- `terms`: attribution text, link-back required (bool, "follow"), apply link unchanged (bool), no redistribution (bool), drop expired (bool), max fetches/day, notes + source URLs of the clauses.
- Seeded (as `approved`, owner decision 2026-10-27) from `crates/modules/sources/job_boards.toml`: **Himalayas** (`/jobs/api`, cursor, `locationRestrictions`/`timezoneRestrictions` → regions, salary fields; ≤ 60/min, hourly), **Jobicy** (`/api/v2/remote-jobs`, `jobGeo` → regions; ≤ 1/hour), **Remotive** (`/api/remote-jobs?category=software-dev`; ≤ 4/day), **RemoteOK** (`/api`; skip the first legal element; `Crawl-delay: 1`), **Arbeitnow** (`remote=true` only; hourly), **HN Who is hiring** (existing extractor). `declined` rows with reasons: Landing.jobs (robots `/api/` disallowed + ToS forbids scraping), The Muse (US-centric, stale), Wellfound/Dice (robots). `candidate` rows needing the owner: We Work Remotely (ToS unread: human must read), Working Nomads (ask permission). Owner 2026-10-27: "i dont care for job boards in portugal … it should look for remote jobs anywhere … they dont have to be dropped but they're not prio". Every board gets a `priority` (`high` = remote-first with worldwide reach, `low` = national/regional or general boards). The seeded approved boards are all `high`. National/regional boards (ITJobs.pt — free key by email, Net-Empregos RSS, Adzuna — free key, personal use only, SwissDevJobs…) are `low`-priority `candidate` rows: kept, listed after the high-priority ones on the Boards page, never blocking anything.

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
3. Result → `candidate` with `check` filled and a `priority` from the check (high when most jobs are remote and open worldwide; low otherwise); the queue is sorted by priority; never fetched again until approved. Re-check approved boards' robots/terms every 6 months (flag changes for the owner).
No model calls in discovery by default; an optional model pass to propose a field mapping for JSON APIs counts against the spend limits.

## Client: Boards page
List by status (Approved, Waiting for you, Declined). A candidate shows the check: endpoint, robots verdict, the terms sentences, sample jobs, proposed attribution — **Approve** / **Decline (with reason)** / **Edit config** (JSON editor for the mapping, validated). Approved boards show last fetch, jobs read, errors, Pause. "Suggest a board" (URL). Pipeline peek shows "From Himalayas ↗" (followed link) for each source.

## Cost
Fetching is free; discovery uses no model by default. Search cost unchanged (model only after filters, top 40).
