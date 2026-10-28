# Boards for your expertise: search approved boards by your top skills, suggest boards for your stack

Issue: https://github.com/notacamp/applicamp/issues/28

## Why
Owner, 2026-10-28: "it should also add job boards more applied to my expertise". He is mostly a
Ruby/Rails/React developer and was mislabeled as a TypeScript developer. Every approved board is
read with one generic query (Jobicy `tag=engineer`, the plain feed elsewhere), so the best 40 jobs
sent to the scorer come from a generic pool, and the seed list holds no board built around a
stack.

## What changes
- **Stack queries.** A board whose API can filter by skill or keyword names it in its config
  (`skill_search`). A search reads the board's plain feed and, for each of the person's top 3
  skills, one extra query; the answers are merged and deduplicated. Verified today: Jobicy `tag`,
  RemoteOK `tag`, Himalayas `q` on its search endpoint. Arbeitnow has no such parameter: it keeps
  its one plain read.
- **Top skills.** The first 3 entries of the profile's `skills`, in the person's own order, mapped
  to each board's vocabulary by a small table in the board's config. No model call.
- **The pre-rank favours the stack**: a job whose title or description names a top skill ranks
  higher, so the stack queries' jobs reach the scorer.
- **Boards for your stack.** Candidates only, with real configs where a feed exists: Python.org
  Jobs, LaraJobs (Laravel), Golangprojects (Go); Ruby on Remote and JavaScript.jobs as candidates
  with no feed yet (the check looks for one). Nothing is approved or declined.
- **"Fits your skills N%"** on the Boards page, next to the salary: the share of a board's listed
  engineering jobs (a candidate's check sample too) that name one of the person's top skills.
  Ranking stays by average salary.
- With no skills, nothing changes.

## Modules touched
`sources` (config, reader, matching, boards API, seed list, one migration), `server` (the profile's
skills reach the search). Client: Boards list and peek, boards API types, 4 languages; the e2e fake.

## Non-goals
- A model call to pick or translate skills; a search by every skill.
- Approving or declining any board: only the owner decides.
- Reading boards whose terms or robots forbid it.
- Re-ranking boards by fit: the order is still the average salary.
