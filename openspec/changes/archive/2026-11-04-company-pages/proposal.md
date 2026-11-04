# Company hiring pages, found through the apply links of jobs already read

Issue: https://github.com/notacamp/applicamp/issues/87 (epic #85, ADR 0010)

Modules: `sources` (the registry, its readers, the daily check, the Boards page in
`clients/app/src/modules/sources`). The admin page that shows waiting rows and acts on them is #86.

## Why
The 107 seeded company boards were switched off in searches and nothing added new ones, yet every job
read names where to apply, and most apply links point at a company's own hiring system. Each one
reveals a company hiring page. ADR 0010: the app does the work first; approvals and control stay.

## What changes
- **Hiring systems are rows of the registry** (kind `hiring_system`): Greenhouse, Lever, Ashby and
  the new Workable, Recruitee, Teamtailor and Personio. Each row holds the system-level check: the
  robots rules of its hosts, its terms read once by the model like any board's, and its
  `Content-Signal`. A clean system is approved by the app. A system whose content signal says
  `ai-train=no` waits for the owner with the new blocker `needs_choice` ("reading its pages is
  allowed; it asks artificial intelligence tools not to learn from them"), and approving it is the
  choice. SmartRecruiters is left out: its robots.txt allows only LinkedInBot.
- **Company pages are rows too** (kind `company_page`): system plus company (unique on both), name,
  the page where its jobs are, how it was found ("an apply link on <site>", "web search", "your
  company watch", "the built-in list of companies") and a reference to its system's row. Every job
  read (a search, a watch, a board's check) has its links matched against the systems' public
  addresses and a new company is registered as a candidate: one batched statement, no request, no
  model.
- **The app approves a company page** when its system is approved, a sample read returns jobs and
  one was posted in the last 60 days: no terms call per company. A page whose system waits stays
  waiting, and a page of a declined system is declined with it.
- **A daily check** (05:00 UTC, and once when new pages were found) checks the systems that are due
  and at most `APPLICAMP_SOURCES__COMPANY_PAGE_CHECKS` new pages a day (300), taken in turns across
  the approved systems, each host kept to the instance's pace.
- **Four new readers**: Workable (widget API), Recruitee (offers API), Teamtailor (job feed) and
  Personio (XML feed), each with a fixture of a real public account and robots findings.
- **Watchers reuse the registry**: a watched company's board is linked to its company page, added
  when it was not there, and recorded as "your company watch".
- **Optional web search** (Brave, `APPLICAMP_SEARCH__BRAVE_API_KEY_FILE`, off without a key): weekly,
  at most 50 queries, within 1000 a month, built from the roles of all accounts' goals and the
  approved systems' hosts; each result link only registers a candidate company page.
- **Searches do not read company pages yet** (`APPLICAMP_SOURCES__SEARCH_COMPANY_PAGES`, off): a job
  store (#89) comes first. Watchers read their companies' boards as before.
- **The Boards list** shows hiring systems and company pages by their kind in plain words, with the
  counts of company pages and the state of the web search.

## What the app does by itself / what waits on the person
By itself: registers company pages from apply links, checks each system once, approves clean
systems and company pages with recent jobs, searches the web within its caps. Waiting on the
person: a system whose content signal asks AI tools not to learn (needs_choice), terms that forbid
or ask first, and the Brave key (a host secret).

## Cost
No model call per company. One terms call per system (seven), again only when its terms change,
about EUR 0.007 once. One sample read a company, within a daily limit. Brave bills each web query:
50 a week and 1000 a month at most, counted.

## Non-goals
Reading company pages in searches (a job store, #89); the admin page (#86); SmartRecruiters; reading
careers pages that describe jobs for search engines beyond what watchers already do.
