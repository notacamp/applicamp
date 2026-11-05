# The check finds a site's job list in more places and knows a posting by its link

Issue: https://github.com/notacamp/applicamp/issues/108 (epic #85, ADR 0010)

Modules: `sources` (the daily check in `features/discovery`, the feed reader, the report). The admin
page that shows waiting rows (#86) reads the new report sentence.

## Why
The first daily discovery approved 2 of 100 sites. Twenty-six candidates had 5 to 20 sample jobs, a
working feed and a clean or silent verdict, and still waited: `postings` counted an item only with a
company or an apply link (19 of the 26 feeds give neither: the company is in the title and the item's
link is the posting page), and `is_a_live_job_board` needed a posted date, which feeds whose config
did not map `pubDate`, `dc:date`, `published` or `updated` do not give. Many sites also have no
config at all because the finder looked only in six places.

## What changes
- **A posting** is a title plus a company, an apply link, or a link to its own page on the site's
  host (a page whose path names a job, never a blog post). A site whose jobs all apply on one
  company's hiring page is that company, not a job site: its company page is registered (#87) and the
  site waits.
- **The company is read from the title** for feeds with no company field: "Role at Company",
  "Company: Role", "Role - Company" (also `|`, em and en dashes, `@`), "Company is hiring a Role". A
  pattern is applied only when at least 70 % of the sample's titles follow it, with three different
  companies and none more than half of the matches. It is kept in the config as `title_split`, so
  searches read the same way.
- **Alive**: proposed feed configs map dates by default (the date element most items carry among
  `pubDate`, `published`, `dc:date`, `updated`). When no item has a date, the feed's `lastBuildDate`
  or `updated`, or the HTTP `Last-Modified`, within 30 days counts. A discovered board nobody edited
  gets the missing date mapping and title split on its re-check.
- **The finder** looks in more places, each address once, robots respected, within a request budget
  (25 after robots.txt and the homepage, `APPLICAMP_SOURCES__DISCOVERY_MAX_REQUESTS`): the feeds the
  homepage links to; a job-board platform recognised from its markup and read at its public address
  (WP Job Manager, Job Boardly, JBoard, Niceboard); the site's own Jobs page and its feed links; the
  sitemaps robots.txt lists, followed through indexes down to the sitemap of jobs, five pages read for
  `JobPosting` JSON-LD; then `/jobs.rss`, `/jobs/feed`, `/feed/jobs`, `/jobs.xml`, `/feed`, `/rss`,
  `/feed.xml`, `/rss.xml`, `/jobs.json`, `/api`, `/jobs/api`, `/?feed=job_feed` and
  `/feed/?post_type=job_listing`.
- **The terms page** is also looked for at `/terms`, `/terms-of-service`, `/tos`, `/legal` and
  `/terms-and-conditions` (a page with a terms heading, not a catch-all page), and through a terms
  link on the Jobs page. The verdict call is unchanged.
- **Every candidate is checked once more with these rules**, oldest check first, within the day's
  100 checks and the AI caps; the terms are not read again unless they changed.
- **Why a candidate waits** is one plain sentence in its report (`waiting_because`), for #86's page.

## What the app does by itself / what waits on the person
By itself: finds the list of jobs in more places, reads companies from titles, judges the site alive
from its dates, registers a single company's page, re-checks the waiting candidates once and
approves the clean ones. Waiting on the person: terms that forbid or ask first, a site that needs a
key, a board the owner edited, a site that is one company, and anything the check could not find
within its request budget.

## Limits
At most 25 requests per check after robots.txt and the homepage (5 to 100 by setting), one request
per address, robots.txt respected on every address, one second between requests to one host, 100
checks a day. No new model call: terms are read again only when their text changed.

## Non-goals
Reading a job list that only exists as a page of HTML with no feed, API or JSON-LD; Jobboard.io (no
public board could be reached to verify it); changing the terms verdict; changing the admin page
(#86).
