# Design

## Data
**`pipeline.opportunities`**: `id uuid pk, account_id, title, company, location text, work_mode text null (remote|hybrid|onsite), url text, source text (greenhouse|lever|ashby|arbeitnow|hn|link), source_ref text, posted_at, salary_text text, description text (≤ 20 000 chars, plain text), match int null (0–100), match_why jsonb (list of short reasons), match_gaps jsonb (list), stage text (found|shortlisted|applying|applied|talking|interview|offer|closed) default 'found', next_step text, notes text, dismissed bool default false, created_at, updated_at`; unique `(account_id, source, source_ref)`; index `(account_id, stage)`.
**`sources.boards`**: `id, kind (greenhouse|lever|ashby), token text, company text, added_by (seed|account uuid)`; unique `(kind, token)`. Seeded from `crates/modules/sources/boards.toml` (a curated list of ~100 companies hiring remotely in Europe or in PT/ES/FR/DE/NL/UK on those ATSs; public board tokens only).
**`sources.searches`**: `id, account_id, started_at, finished_at, status, fetched int, kept int, scored int, added int, error_kind`.
Events: `pipeline.opportunity_added` v1 `{ account, opportunity }`, `sources.search_finished` v1 `{ account, search, added }`.

## Reading the boards
- Greenhouse `https://boards-api.greenhouse.io/v1/boards/{token}/jobs?content=true`, Lever `https://api.lever.co/v0/postings/{token}?mode=json` (EU: `api.eu.lever.co` when the token is EU), Ashby `https://api.ashbyhq.com/posting-api/job-board/{token}?includeCompensation=true`, Arbeitnow `https://www.arbeitnow.com/api/job-board-api?page=N` (≤ 5 pages), HN: the latest "Ask HN: Who is hiring?" via Algolia (`hn.algolia.com/api/v1/search_by_date?tags=story,author_whoishiring`) and its top-level comments.
- One HTTP client with a 10 s timeout, a 2 MB body cap, identify as `applicamp/<version>`, ≤ 2 requests/s per host, responses cached 6 hours (`sources.cache(url pk, fetched_at, body bytea)`). HTML descriptions become plain text (strip tags; no scripts kept).
- Normalisation per source into one `Job { source, source_ref, title, company, location, work_mode?, url, posted_at?, salary_text?, description }`. HN comments have no structure: the model extracts `{company, title, location, work_mode, url}` per comment in the scoring call; comments it can't read are skipped.

## Choosing and scoring (one search)
1. **Hard limits from goals** (structured fields only, no text heuristics on the job's prose): drop jobs older than 30 days, jobs already in the pipeline or dismissed, and jobs whose structured location/work mode clearly conflicts with `work_mode` + `locations` + `relocate` (e.g. onsite in a city not listed and relocate = no). Anything ambiguous goes to the model.
2. **Pre-rank** the rest cheaply by title similarity to `goals.roles` (token overlap) and recency; keep the top 80.
3. **Score** with the `complete` role in batches of 10: input = a compact profile summary (headline, roles held, skills, years, languages) + goals + each job's title/company/location/salary/first 1 500 chars of description; output JSON per job `{ ref, match 0–100, why: [≤3 short reasons citing the profile], gaps: [≤2], work_mode?, salary_text? }`. Job text is untrusted data (the prompt says so; output parsed strictly).
4. **Keep** `match ≥ 65` (configurable per account later) as **Found**, at most 25 per search. Searches are capped at 1 per 15 minutes per account and use the account's daily token cap.
5. A search runs as a background job (`sources.search`), with progress events on the search row for the UI (fetched / scored / added). Nightly at 21:00 Europe/Lisbon for accounts with goals.

## Add by link
`POST /api/v1/opportunities { url }`: if the URL is a Greenhouse/Lever/Ashby posting on a known pattern, fetch it through the same reader (and add its board to `sources.boards` for future searches); otherwise create an opportunity with the URL and let the person type title and company. The server never fetches arbitrary hosts (SSRF): only the three ATS API hosts.

## Endpoints (session; OpenAPI)
`GET /api/v1/opportunities?stage=&q=`, `GET /api/v1/opportunities/{id}`, `POST /api/v1/opportunities`, `PATCH /api/v1/opportunities/{id}` (stage, next_step, notes, dismissed, title/company/location for link-added ones), `POST /api/v1/searches` → 202 search, `GET /api/v1/searches/latest`, `GET /api/v1/today` → `{ profile: {done, strength_hint}, goals: {answered, total}, new_matches, latest_search, next_step }`.

## Client
- **Pipeline** (prototype "Pipeline" page): views "All" (table: role, company, match bar, stage tag, source tag, next step, updated) and "By stage" (board); filter by stage, search box; "New" (paste a link); "Search now" with live progress; side peek (properties, why it matches, gaps, description excerpt, open posting, stage select, notes, Not for me). 375px: list cards + full-screen peek.
- **Today**: state-aware. No profile → set up profile. Profile but goals unanswered → answer goals. Both done, no search yet → "Search now". After searches → "N new matches" linking to Pipeline filtered to new ones, the latest search line ("Last night: 412 jobs read, 9 matched"). The "Nothing needs you yet" copy only when there truly is nothing.
- 4 languages, keyboard (⌘K "Search now", "New opportunity").

## Testing
Readers against recorded fixtures (wiremock) per source; hard limits; pre-rank; scoring with a fake complete endpoint (incl. a hostile description trying to inject instructions — the output still parses and the job is just scored); dedupe; add-by-link incl. a refused non-ATS host; e2e: Search now with fake sources/model → Found rows appear → open peek → move to Shortlisted (desktop + phone).
