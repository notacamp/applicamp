# Design

- **Data** (`sources.job_boards`, migration `sources__company_pages`): `kind` gains `hiring_system`
  and `company_page`; new `system`, `company_token`, `eu` and `system_id` (a page's system row, same
  schema, cascade). `host` stays unique except for company pages; `system` is unique for system
  rows; `(system, lower(company_token), eu)` is unique for company pages. `sources.companies.page_id`
  links a watched company to its page, and its `board_kind` gains the four systems.
  `sources.web_search_queries` counts each query asked (query text, links returned, pages added).
- **One registry, two kinds that are never searched**: `job_boards::approved` (what a search reads)
  excludes them, their `config` is empty and `PUT .../config` refuses them. Discovery's queries skip
  them too; "Check again" on one runs its own check.
- **Readers** (`readers/{workable,recruitee,teamtailor,personio}.rs`) give the same `Job` as the
  three existing ones, through `Reader::board`, at addresses from `APPLICAMP_SOURCES__*_URL` (the
  three subdomain systems take `{token}` in the address). Verified 2026-11-04 with public accounts:
  Workable `GET apply.workable.com/api/v1/widget/accounts/<t>` (robots empty `Disallow`, content
  signal `ai-train=no`); Recruitee `GET <t>.recruitee.com/api/offers` (robots disallow `/v/`);
  Teamtailor `GET <t>.teamtailor.com/jobs.rss` (robots disallow `/app/` and others, content signal
  `ai-train=no`); Personio `GET <t>.jobs.personio.de/xml` (robots `Allow: /`).
- **Matching** (`hiring_systems::company_link`) is the one place the public addresses are known:
  `boards.greenhouse.io/<t>`, `job-boards.greenhouse.io/<t>`, `jobs.lever.co/<t>`,
  `jobs.eu.lever.co/<t>`, `jobs.ashbyhq.com/<t>`, `apply.workable.com/<t>`, `<t>.recruitee.com`,
  `<t>.teamtailor.com`, `<t>.jobs.personio.de|com`. The watchers' `hint_of` falls back on it. It is
  called where jobs are read: `Reader::board` (company boards, watchers) and `Reader::job_board_for`
  (registry boards), and on a check's sample. `company_pages::register` is one `INSERT ... SELECT
  FROM unnest` with `ON CONFLICT DO NOTHING`; a failure is logged, never raised.
- **System check** (`features/company_pages::check_system`): robots.txt of each host the system reads
  (its API host and where its job pages are; with `ALLOW_PRIVATE_HOSTS`, the reader's own address),
  every path read allowed; the `Content-Signal` merged (a `no` wins); the homepage and the terms
  page where robots allows; one `complete` call for the terms (role `complete`, system account,
  the spending limits of `applicamp-ai`, fenced as data, kept until the terms sentences change; no
  call for a site with no terms page or no sentence about automated reading, and not configured
  leaves the system waiting with "its terms could not be read yet"). Stored blockers: a
  `robots_disallowed` fact and `needs_choice`. Approved by the app only through the guarded
  `UPDATE ... WHERE status = 'candidate'` when `blockers()` is empty and the verdict is fresh and
  allow or silent. Declined and paused systems are never touched; an approved one is checked again
  after 182 days and only flags a change.
- **`needs_choice` is a choice, not a wall**: `blockers()` lists it, `approve` and `approvable` ignore
  it (`CHOICES`), so the admin page shows why it waits and Approve works; a model's `forbid` or
  `ask_first` still blocks. A page cannot be approved before its system (`system_waiting`).
- **Declining a system declines its pages** in the same call; a page registered for a declined
  system is born declined.
- **Daily run** (`sources.check_company_pages`, `sources.check_company_pages_soon`): systems due
  first; then, per approved system whose robots rules still allow reading, its candidate pages not
  read or whose retry is due (30 days after a read that showed no recent job, 7 after a failure);
  taken in turns across systems until the day's limit (counted from `report.checked_at`) is spent;
  a 429 or 503 stops that system for the day. One sample read is `Reader::board`, at most
  `request_spacing` between two requests to a host.
- **Web search** (`features/company_pages/web_search.rs`): off without a key file; the key is read at
  run time and never logged. Queries are every role (lower case, plain words) times every approved
  system's search host, in a fixed order, a window of 50 moving along them week after week, cut to
  what the month still allows. Each query is counted whether Brave answered or not. A 401, 402, 403
  or 429 stops the run. Only links on a system's public address are kept.
- **API**: `BoardSummary` gains `system` and `company_token`; `BoardList` gains `company_pages`
  (counts) and `web_search` (on, queries this month, monthly limit). The list holds every board
  and system and at most 100 company pages, the waiting ones first. Scope as before: signed in to
  read, board owners to decide.
- **Fakes**: in the e2e stack every system is read at `<system>.hiring.test`, an alias of the fake;
  clean-site.test's jobs carry apply links on three systems' public addresses; Lever's fake robots
  says `ai-train=no`.
