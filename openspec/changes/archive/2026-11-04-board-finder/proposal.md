# Boards approve themselves when their check is clean, and discovery reads more lists

Issue: https://github.com/notacamp/applicamp/issues/75 (epic #85, ADR 0010)

Module: `sources` (the Boards page in `clients/app/src/modules/sources`). The admin page that asks
boards for permission is #86; it reads the fields this change adds.

## Why
36 of 41 boards waited for the owner one by one, including boards with a clean check. Discovery
read one list and checked 20 boards a week, and once proposed CV tools as job boards. Nothing said
what kind of jobs a board has. ADR 0010: the app does the work first; approvals and control stay.

## What changes
- **The app approves a clean board** (a candidate nobody edited, from discovery, the seed list or a
  suggestion): its config reads sample jobs, robots rules allow every path read, at least 5 sample
  items look like postings (a title and a company or apply link), it posted in the last 30 days,
  its terms allow apps or say nothing, no blocker and no key needed. It records `approved_by =
  'app'` and the date; the peek says "Approved by the app on <date>". A board the owner paused,
  declined or edited is never touched; a model that cannot be asked (not configured, spending
  limit) leaves the board waiting.
- **Terms read by a model**: one `complete` call per board (again when its terms change), charged
  to the instance budget as the system account, refused past a limit and never retried. It returns
  allow, silent, forbid, ask first or personal only, with the sentence that decided it (checked to
  be one of the sentences it was given). Forbid is the `terms_forbid` blocker; ask first, personal
  only and a forbid that the site's own robots rules contradict by an explicit `Allow` wait for the
  owner as the new `needs_permission` blocker, with the quote. No terms page found is silent, with
  no call for the verdict. Terms text is fenced as data.
- **Tags** from the same call, from the sample jobs: kinds of jobs (fixed list), where, how remote,
  and specialities.
- **Content-Signal** line of robots.txt parsed and kept (`ai-train=no` never blocks reading; the
  app never trains on jobs); shown in the peek.
- **Discovery reads six public lists** (CC0 and MIT, each credited as "found through"): the
  existing one, awesome-remote-job (numbered items), remote-working-list (CSV with feed
  addresses), awesome-remote-jobs, idimetrix/awesome-job-boards and Wikidata's employment
  websites. Up to 200 boards a run (`APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS`); it stops cleanly
  at an AI spending limit.
- **Seeds** 4dayweek.io (cents divided by `salary_divisor`), Remote First Jobs (one feed; its
  API is disallowed), RemoteYeah, Real Work From Anywhere, the Rails job board, Elixir Jobs and
  Get on Board as candidates the check approves; Jobgether, VueJobs and Elixir Forum as
  `needs_permission` with their quote and source; Startup.jobs needs a key. HN jobstories is not
  seeded: the readers cannot express "list of ids, then one request each".
- **Lenient feeds**: tags in any case, a byte-order mark, a bare `&`, and an unclosed item no
  longer lose the feed.

## What the app does by itself / what waits on the person
By itself: finds boards on six lists, checks them, reads their terms, tags them, approves the
clean ones. Waiting on the person: boards that need permission, a key or a choice; pausing or
declining any board; anything the owner edited.

## Cost
One terms call per new board, plus one when its terms change: about EUR 0.001 each on GLM-5.3
Flash, about EUR 0.20 for 200 boards a week. Charged to the instance budget; at the limit
discovery stops and the rest waits for the next run. The calls count on the system account's
daily token cap like any account's.

## Non-goals
Permission requests and the admin page (#86); the marketplace page; reading Discourse forums,
the Workable network feed or HN jobstories.
