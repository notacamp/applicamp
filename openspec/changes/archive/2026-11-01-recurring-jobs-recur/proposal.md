# Nightly searches and weekly discovery recur on their own

Issue: https://github.com/notacamp/applicamp/issues/55

## Why
No nightly search ran on 10-29 or 10-30 on the hosted instance, and board discovery ran only
three times in a week. Both jobs schedule their next run from inside their own run with a
unique enqueue that refuses while a job of the kind is waiting **or running**, so they never
added it. They recurred only when a deploy restarted the server (its start-up schedule added one).

## What changes
- `kernel::jobs::enqueue_next_at`: the next run of a recurring job, unique among the jobs still
  waiting; the one running does not count. The start-up schedule keeps `enqueue_unique_at`.
- The nightly search and the weekly board discovery schedule their next run with it.

## Not changing
- When they run (21:00 Lisbon, weekly), what they do, and the one-off start-up jobs.
