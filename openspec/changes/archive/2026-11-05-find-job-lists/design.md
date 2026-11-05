# Design

- **Data**: none new. `sources.job_boards.report` (JSON) gains `waiting_because` (one sentence, empty
  once approved) and `rules` (the version of these rules, `discovery::RULES`); both are in
  `BoardReport` of the OpenAPI document. No migration.
- **Pure parts, tested alone**: `discovery/postings.rs` (posting evidence, title patterns),
  `discovery/waiting.rs` (the sentence), `finder.rs` helpers (platform markup, `Sitemap:` lines, Jobs
  page link, terms heading). The title split itself is `readers::mapping::split_title`, the same
  function `job_of` applies for a config's `title_split`.
- **Request budget**: `Visits` counts every request after the homepage and refuses past the budget,
  recording that it did (`exhausted`, shown as "The check used its N requests before it found where
  the jobs are"). Order: the feeds the homepage links to, the platform's address, the Jobs page and
  its feeds, the terms page (link, Jobs page link, usual paths), the sitemaps, the common paths. The
  terms page comes before the blind paths so a spent budget never silences a site's terms.
- **Platforms** (verified 2026-11-05 against public boards): WP Job Manager at `/feed/job_feed/`
  (jobspresso.co; the pretty address has no query, which many robots.txt files disallow), Job
  Boardly at `/jobs.xml` (motorsport.jobs, otjobsaustralia.com.au; `<job>` elements), JBoard at
  `/rss/jobs` (golf-jobs.com; most JBoard sites disallow `/rss/` in robots.txt, so the sitemap is
  read), Niceboard through its sitemap index (bot-jobs.com; no public feed). Jobboard.io: no public
  board answered a plain request, so it is not recognised.
- **Alive**: `posted_recently(sample, now, fresh_at)`; `fresh_at` is the feed's `lastBuildDate` or
  `updated`, else `Last-Modified` (`Reader::visit_noting` returns the validators of a page).
- **Re-check queue**: candidates with a report, no blocker and `rules < RULES`, oldest `checked_at`
  first, after the new and never-checked candidates, until the day's `discovery_max_checks`.
- **Leases**: the daily discovery and the seeded check hold their lease for 90 minutes (a hundred
  checks of up to 25 requests, a second apart).
- **Safety**: every address passes `allowed` (robots.txt rules of the site) and the reader's public
  host check; a catch-all page cannot pass as a terms page (heading test) or a feed (it must parse).
