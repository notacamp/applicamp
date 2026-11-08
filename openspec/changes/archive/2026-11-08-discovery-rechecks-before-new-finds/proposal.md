# Discovery re-checks the waiting sites before new finds use up the day's checks

Issue: https://github.com/notacamp/applicamp/issues/113
Module: `sources` (`features/discovery`, the `discovery_runs` table). No endpoint changes.

## Why
Found in prod on 2026-11-08 by reading the runs of 11-06, 11-07 and 11-08. Each daily run checks 100
sites and the public lists offer 100 new hosts every day. `discover()` checked the new finds first,
until the day's checks were spent, so the two later passes never ran: candidates never checked
(suggested or seeded) and candidates checked under older rules (#108). The 104 candidates checked
before #109 stayed on their old report, among them 26 sites with good samples that had failed the
old `postings()`. The spec said the same ("after the new and never-checked candidates").

## What changes
- **Order of the day's checks.** Never-checked candidates first, then candidates checked under older
  rules (oldest check first), then new finds from the lists, all within
  `APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS` (100) and the AI caps.
- **Room for new finds.** The first two passes together take at most half of the day's checks
  (`max / 2`, so 50). New finds always get at least the other half and whatever the first two leave.
  With no old work, they get all 100.
- **Re-checks are counted.** `discovery_runs.rechecked_candidates` (new column) and the run's log
  line count the candidates checked again under new rules. `rechecked` keeps meaning the approved
  boards due a re-check.

## Automates, and what still waits on the person
The app already ran these checks by itself; only their order changes. Nothing new waits on the
person, and approvals still follow the same clean-check rules.
