# The check stops asking a site that says slow down, and says so

Issue: https://github.com/notacamp/applicamp/issues/117
Module: `sources` (`features/discovery`, the waiting reasons). One new waiting code,
`asked_to_slow_down`; the report is jsonb, so no schema change. `RULES` stays 2.

## Why
In the 2026-11-10 discovery run, 21 sites answered 429 to every request, starting with robots.txt: 16
requests each, 336 in all, all wasted. The check logged it and went on to the next path on the same host, and
the site then waited as `no_job_list`, which hid the real reason.

## What changes
- During a check, a host that answers 429 or 503 (or the reader's back-off for it) gets no more requests; its
  later pages cost no budget; one log line per host. Other hosts are still read.
- A candidate stopped this way with no list found waits as `asked_to_slow_down` (before `request_limit`),
  worded in en, pt-PT, es-ES and fr. A list found first keeps its usual reason.
- A small data migration recodes the 21 prod candidates (public job-site names) whose 2026-11-10 check got
  only "slow down" answers from `no_job_list` to `asked_to_slow_down`, so they say why without a re-check.
  On a database without these rows it changes nothing.

## Automates, and what still waits on the person
The app stops asking by itself. It does not retry on its own; the owner can press "Check again".
