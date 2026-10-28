# Design

## Data (schema `sources`, module `sources`)
- `sources.job_boards` gains `blockers jsonb` (`[{code, detail, source}]`: `terms_forbid` or
  `robots_disallowed`, the research's facts, refreshed from the seed list at every start) and
  `api_key bytea`, `api_key_host text`, `api_key_set_at timestamptz`
  (`20261028120000_sources__board_keys_and_blockers.sql`).
- `20261028120100_sources__seeded_declines_wait.sql`: a seeded row still `declined` with the
  seed's own reason (so the owner never decided it) becomes `candidate`, `decline_reason`
  cleared, the research as `report.notes`, facts as `blockers`. A row the owner declined (any
  other reason) is untouched. Idempotent.
- The report gains `proposal` (`{kind, config}`) and `config_fingerprint` (the config the check
  read with). No event is published or consumed.

## Seed list
- `job_boards.toml` declines nothing; `parse` turns an older list's `declined` board into a
  candidate with its reason as the note. `blockers` may state only `terms_forbid` and
  `robots_disallowed`, each with a detail. `sync` refreshes the blockers and the note (never the
  status, never an edited config).
- Research of 2026-10-28 (one GET each): SwissDevJobs `GET /api/jobsLight` (JSON list, ~250
  jobs; `_id`, `name`, `company`, `jobUrl` → `/jobs/<jobUrl>` through the new `link_base`,
  `actualCity`, `workplace` → work mode through the new `fields.work_mode`,
  `annualSalaryFrom/To` in CHF, `activeFrom`); robots.txt disallows `/api/` only for Meta's
  crawler. Net-Empregos `GET /rss.asp` (ISO-8859-1 RSS 2.0, 1 000 items, `dc:creator` the
  company); robots.txt allows it. ITJobs.pt answers "API key required." (key by email on its
  API page). Adzuna needs `app_id` + `app_key`; its API host's robots.txt disallows everything
  for every crawler (the check reports it). Landing.jobs' ToS item (vii) and robots
  (`/api/`, `/jobs/search`); Wellfound's robots (`/_jobs/`, `/search`, `?jobId=`, `?jobSlug=`)
  and ToS ("harvesting, collection or 'scraping'"); Dice's robots (`/job`, `/jobsearch/`,
  `/rss/`, `/feed/`) and ToS (j), (aa). The Muse's public API is allowed by its robots.txt.

## Approval (`blockers(row)`, in order)
1. the row's research blockers; 2. `needs_key` when the config names a key and none is set for
the host it reads; 3. `no_endpoint` (no feed, API or sitemap address); 4. `config_invalid` (the
validation message, naming the field); 5. `check_needed` (no check of the feed's host); 6.
`robots_disallowed` (the checked rules forbid the feed's path, with the deciding rule).
`BoardDetail` carries `blockers` (code, detail, source), `blocked` (the first code, as before)
and `key` (`params`, `how`, `url`, `set`, `set_at`; never the key). `POST …/approve` answers 409
with the first blocker's code and detail.

## Keys
- `config.key = {params, how, url}`; validation refuses a key parameter written in `query`.
- `PUT /api/v1/boards/{id}/key` `{values: {param: value}}` (owner; each value 1–200 visible
  characters; 400 `invalid_key` names the parameter; 409 `no_key_needed` / `no_endpoint`) and
  `DELETE /api/v1/boards/{id}/key` (owner). Scope: the session, board owners only (403
  `not_board_owner`).
- Sealed with kernel `crypto::Cipher` under the purpose `applicamp sources board key v1`,
  bound to the board's id. `SourcesDeps::with_secret` (composed in `main`) gives the reader and
  the check the same cipher. The key is added to the request's query only: the cache, logs,
  errors, scans and the report know the address without it; it is sent only to
  `api_key_host`. Another instance secret cannot open it (the board then needs a key again).

## Config errors
`BoardConfig::from_json` reads the config; on failure it deserializes each key alone (and each
`fields` key) to name the first one that fails: `` `max_pages`: invalid type… ``, `` `evil` is
not a config key ``. `validate` names every field (`` `fields.url` is required… ``).

## The check
- Its own address first, read with its own config (`read_own`), with its key; never asked when
  the key is missing or the host's robots rules disallow it; feeds up to 4 MB.
- No usable config: probes the site's RSS links (2), `/api`, `/jobs/api`, `/feed`, `/rss`, then
  `/sitemap.xml` (its pages naming a job, on the site, allowed; three read for JSON-LD
  `JobPosting`s; the shared path as `job_path`). The first that reads is the sample and the
  proposal: applied by itself only to a discovered or suggested board nobody edited (as
  before), otherwise kept as `report.proposal`. `PUT …/config` takes an optional `kind` (an
  approved board keeps its kind: 409 `kind_locked`) and clears the proposal.
- `sources.check_seeded_boards`, enqueued at every start (unique): seeded candidates without
  research blockers, never checked, or checked with another config (fingerprint), at most 20,
  as one discovery run. The weekly discovery also skips boards with research blockers.

## Client
The peek's owner section: the blockers (`board-blocked`, one `board-blocker-<code>` each,
`unknown` for a code it does not know, with a "Where it says so" link), Approve / Pause /
Check again with their errors right under them (`board-<action>-error`: the translated
message and the server's detail), the key (`board-key`, one password field per parameter,
"Key set", "Get a key", "Remove key"), the proposal (`board-proposal`, field → source, "Use this
config"), Decline, and "Advanced" (`board-config-advanced`) with the JSON editor and its error
beside Save (`board-config-error`). 4 languages.

## AI
None.
