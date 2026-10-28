# Design

## Data (schema `sources`, module `sources`)
- `sources.board_jobs` gains `words text[] NOT NULL DEFAULT '{}'`
  (`20261028140000_sources__board_job_words.sql`): the distinct lower-case words of a listed
  job's title and the start of its description (at most 200), so a board's fit can be computed at
  read time for whoever looks. Rows from before are empty and ignored by the fit.
- No event is published or consumed. No endpoint is added; `BoardSummary` gains `skills_fit`
  (`{share, jobs}`, absent without skills or listed jobs). Scope: the session, as for the list.

## Top skills (decided here)
The first 3 non-blank entries of the profile's `skills`, in the person's order (they can reorder
them in Profile), deduplicated case-insensitively. Simple, predictable, no model. The same 3 are
used for the stack queries, the pre-rank bonus and the fit label.

## Per-board vocabulary
`config.skill_search = { param, separator, aliases, endpoint? }`:
- a skill is lower-cased and trimmed; if `aliases` has it, the alias is used; else its spaces
  become `separator` (`-` by default; `" "` for a free-text keyword). Example: RemoteOK tags are
  `ruby`, `react`; `rails` returns nothing, so `{"ruby on rails" = "ruby", "rails" = "ruby"}`.
- `param` is the query parameter; `endpoint` is a separate search address (Himalayas:
  `/jobs/api/search?q=`, whose answer has the same fields as `/jobs/api`).
- Verified 2026-10-28 (a GET each): Jobicy `tag` ("Keyword search applied to the available job
  content", https://jobicy.com/jobs-rss-feed); RemoteOK `?tag=ruby` (55 jobs, `?tag=rails` 0;
  https://remoteok.com/api); Himalayas `GET /jobs/api/search` with `q`, 20 per page, `page`, no
  cursor (https://himalayas.app/api); Arbeitnow documents only `visa_sponsorship`
  (https://www.arbeitnow.com/blog/job-board-api): `search=`/`tag=` returned the unfiltered list,
  so it has no skill query. HN and Remotive: none.

## Reading
`Reader::job_board_for(…, skills)`: the plain read as before; then, per stack query, the same
config with `query[param] = value`, `cursor` and `page_param` dropped and one page (Himalayas
search uses `endpoint`, 1 page of 20). Jobs are merged and deduplicated by `source_ref`. A stack
query that fails is logged and stops the rest; the plain read's jobs are kept. Without skills, or
for a board with no `skill_search`, the read is exactly as before.

## Cost
No new model call. The stack queries are plain HTTP fetches, cached like every board read (by
address, for the board's `poll_minutes`) and spaced by its `min_interval_ms`. A board that states
`max_fetches_per_day` (Jobicy: 24) keeps to it: with N extra queries each address is reused for
`max(poll, 1440 * (1 + N) / max_fetches_per_day)` minutes (Jobicy: 4 hours). The scorer's caps
stay: pre-rank top 40, batches of 10, keep at least 65, at most 25, scores cached per
`rules_version`. The fit label and the pre-rank bonus are string comparisons on words.

## Pre-rank
`MatchInput.skills` (the profile's skills in order, at most 40). A job's skill words are those of
its title and description (`matching::words`); a skill matches when all its words are present
(`Ruby on Rails` is `ruby` + `rails`). Bonus: `0.3 * matched top skills / top skills`, next to the
title fit (up to 1), freshness (0.2) and place (0.15). Hard limits and the rules are unchanged.

## Boards for your stack
Researched 2026-10-28 with robots.txt, a feed GET and a search for terms:
- **Python.org Jobs**: RSS `https://www.python.org/jobs/feed/rss/`; robots disallows only
  `/~guido/orlijn/` and `/webstats/` for everyone. Candidate with config.
- **LaraJobs** (PHP/Laravel): RSS `https://larajobs.com/feed`; robots `Disallow:` nothing.
  Candidate with config (`job:company`, `job:location`, `job:salary`).
- **Golangprojects** (Go): RSS `https://www.golangprojects.com/rss.xml`; robots disallows only
  Yahoo Pipes and AhrefsBot. Candidate with config (title `Role @ Company`).
- **Ruby on Remote** (Ruby/Rails): robots allows reading, but every feed and API address answers
  a Cloudflare challenge (403) to a program. Candidate with no config; a note, not a blocker.
- **JavaScript.jobs** (JavaScript/React/Node): robots allows everything, a sitemap exists, no
  feed (404 on `/rss`, `/feed`). Candidate with no config; the check looks for JSON-LD.
- Vue Jobs' robots disallows `/feed`; not added (no other address found).
For all of them the terms were not found or not read: the note says a person reads them before
approving (no board has a blocker the research did not read in a source).

## Fits your skills
`BoardSummary.skills_fit = {share, jobs}`: of the board's listed engineering jobs with words
(sample jobs of a check included), the share naming any top skill. Computed per request from the
signed-in person's profile; the Boards page shows "fits your skills N%" next to the salary.
