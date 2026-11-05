## MODIFIED Requirements

### Requirement: New job boards are found, checked and approved by the app when the check is clean
The app SHALL look for new job boards every day on public lists, check each one's robots rules,
terms and machine-readable feed automatically, and SHALL approve a board by itself, recording
"approved by the app" with the date, only when its check is clean: its config reads sample jobs,
its robots rules allow every path read, at least 5 of the sample items are postings (a title and a
company, an apply link, or a link to the job's own page on the site), the jobs do not all apply on
one company's hiring page, it is alive (a job posted in the last 30 days, or, when no job says when,
the feed's own date or the date the site answered with, that recent), its terms allow apps or say
nothing about automated reading, it has no blocker and it needs no key. A board the owner paused,
declined or edited SHALL never be approved by the app. Only the owner SHALL decline a board: the app
SHALL NOT decline one on the owner's behalf, and what its research found SHALL be shown as a note (an
opinion) or as a blocker (a fact: terms that forbid automated reading, robots rules that disallow the
job pages, a need for permission), never as a decline.

#### Scenario: A board found by discovery with a clean check
- **WHEN** discovery finds a board with a feed of at least 5 postings from the last 30 days, allowed by its robots rules, with terms that say nothing about automated reading
- **THEN** the app approves it, its peek says "Approved by the app on" the date, and searches read it

#### Scenario: Not a job board
- **WHEN** a found site's sample has fewer than 5 items that are postings
- **THEN** it waits for the owner with its check, and the app does not approve it

#### Scenario: A board whose robots rules forbid the feed
- **WHEN** a candidate's robots.txt disallows its API path
- **THEN** the check says so and the board cannot be approved without editing its config to an allowed path

#### Scenario: A board the owner never declined is never shown as declined
- **WHEN** a board was declined by the seed list rather than by the owner (Landing.jobs, The Muse, Wellfound, Dice)
- **THEN** it waits for the owner as a candidate, with the research as its note, and the Declined section lists only boards the owner declined

#### Scenario: Terms that forbid automated reading
- **WHEN** a waiting board's terms forbid automated reading (Landing.jobs)
- **THEN** its peek quotes the clause with a link to the terms, Approve stays disabled, and its site is not read by the checks at start

#### Scenario: A board the owner edited
- **WHEN** the owner edited a candidate's config and a check finds it clean
- **THEN** the app does not approve it

### Requirement: Every waiting board can be approved or says why
A board waiting for the owner SHALL either be approvable or show every
reason it is not, in order, each with what exactly (the terms' sentence, the
robots rule, the field, how to get a key): its terms forbid automated
reading, its robots rules disallow it, it needs an API key, it has no feed
address yet, its config does not read, it was not checked yet. A reason the
client does not know SHALL still be shown, by its detail. Seeded boards
waiting for the owner SHALL be checked at start once with their config (one
request per path, robots respected), unless a fact forbids reading them.
A candidate the check did not approve SHALL also carry, in its report, the reason it still waits
as one plain sentence that does not use the words RSS, robots or API.

#### Scenario: A seeded board after its check
- **WHEN** the server starts with SwissDevJobs waiting and never checked
- **THEN** it is checked once with its own config, sample jobs appear, and Approve is enabled

#### Scenario: A board with no feed address
- **WHEN** a waiting board has no config that reads
- **THEN** its peek says it has no feed address yet and that Check looks for one

#### Scenario: The reason in a sentence
- **WHEN** a check found a feed whose newest job is from three months ago
- **THEN** the report says "Its newest job is from" that date "more than 30 days ago.", and an approved board's report says nothing

### Requirement: The check proposes a config
When a board has no usable config, its check SHALL look for the site's list of jobs, and SHALL
propose what reads as an endpoint and a field mapping, shown as each field and where it is read
from, with "Use this config". The raw JSON config SHALL stay available under "Advanced". A proposal
SHALL apply by itself only to a discovered or suggested board nobody edited. A proposed feed config
SHALL map the item dates (the element most items carry among `pubDate`, `published`, `dc:date` and
`updated`). Where it looks is in the requirement "The check looks for a site's list of jobs in more
places, within a request budget".

#### Scenario: A site with an RSS link
- **WHEN** a waiting board with no config has a homepage linking its RSS feed
- **THEN** its check proposes that feed with title, link, description and date mapped, and "Use this config" makes it approvable

## ADDED Requirements

### Requirement: A posting is a title with a company, an apply link or its own job page
A sample item SHALL count as a posting when it has a title and a company, an apply link, or a link to
its own page on the site's host (or its feed's host) whose path names a job. A link to a blog post
SHALL NOT count, and an item linking to a blog post SHALL NOT count even with an author as its
company. A site whose sample jobs all apply on one hiring system's page for the same company SHALL
register that company page through the company pages' function, SHALL NOT be approved by the app
as a job site, and SHALL say so in its waiting sentence.

#### Scenario: A feed with no company and no apply link
- **WHEN** a feed's items have a title and a link to `/job/…` on the site
- **THEN** they count as postings

#### Scenario: A blog
- **WHEN** a feed's items link to `/blog/…` pages
- **THEN** none counts, and the site waits, saying how many items look like jobs

#### Scenario: One company
- **WHEN** every sample job applies at `jobs.lever.co/acme-labs/…`
- **THEN** the Acme Labs company page is registered, the site is not approved, and it says every job applies on one company's hiring page

### Requirement: The company is read from the titles when a pattern is consistent
For a feed with no company field the check SHALL read the company from the titles when at least 70 %
of the sample's titles follow one pattern ("Role at Company", "Company: Role", "Role - Company",
"Role | Company", "Company is hiring a Role", also with en and em dashes and `@`), with at least
three different companies, none of them in more than half of the matches. A pay in parentheses at the
end of the company SHALL be dropped. A pattern that is not that consistent, a company name over five
words, or the same name after every title SHALL NOT be applied. The pattern SHALL be kept in the
proposed config so later reads give the same company.

#### Scenario: Role at Company
- **WHEN** a feed's titles are "Rust Engineer at Example Labs ($45k-$60k)" and five more like it
- **THEN** the company is "Example Labs" and the title "Rust Engineer"

#### Scenario: The site's name after every title
- **WHEN** every title ends " - Board Jobs"
- **THEN** no company is read

### Requirement: A site is alive by its dates
A site SHALL be alive when a sample job was posted in the last 30 days. When no sample job has a date,
the feed's own date (`lastBuildDate`, or `updated`) or the `Last-Modified` the site answered with
SHALL count when it is within 30 days. A feed whose jobs are dated and old SHALL NOT be alive by its
own date.

#### Scenario: A feed with no item dates
- **WHEN** a feed's items have no date and the site answered with a `Last-Modified` two days old
- **THEN** the site is alive

#### Scenario: Old and undated
- **WHEN** the items have no date and the newest change is three months old
- **THEN** the site waits, saying none of its jobs says when it was posted

### Requirement: The check looks for a site's list of jobs in more places, within a request budget
When a board has no usable config, the check SHALL look for its list of jobs in this order, asking each
address once and only where the site's robots rules allow: the feeds the homepage links to; the public
address of a job-board platform recognised from its markup (WP Job Manager at `/feed/job_feed/`, Job
Boardly at `/jobs.xml`, JBoard at `/rss/jobs`, Niceboard through its sitemap); the site's own Jobs
page linked from the homepage and the feeds it links to; the sitemaps robots.txt lists, else
`/sitemap.xml`, followed through sitemap indexes down to the sitemap of jobs, with five of its pages
read for `JobPosting` JSON-LD as the sample; then `/jobs.rss`, `/jobs/feed`, `/feed/jobs`,
`/jobs.xml`, `/feed`, `/rss`, `/feed.xml`, `/rss.xml`, `/jobs.json`, `/api`, `/jobs/api`,
`/?feed=job_feed` and `/feed/?post_type=job_listing`. One check SHALL make at most 25 requests after
robots.txt and the homepage (`APPLICAMP_SOURCES__DISCOVERY_MAX_REQUESTS`, 5 to 100), and when it
stops for that reason its waiting sentence SHALL say so.

#### Scenario: A sitemap index
- **WHEN** robots.txt lists a sitemap index naming `sitemap-jobs.xml`, `sitemap-companies.xml` and `sitemap-blog.xml`
- **THEN** only the sitemap of jobs is read, five of its job pages are read for JSON-LD, and the config points at that sitemap with the jobs' path

#### Scenario: A platform whose feed the rules disallow
- **WHEN** a JBoard site's robots.txt disallows `/rss/`
- **THEN** its feed is never asked for and its sitemap is read instead

#### Scenario: The budget
- **WHEN** nothing is found and the budget is 7
- **THEN** the site got robots.txt, its homepage and 7 more requests, and the report says the check used its 7 requests

### Requirement: The terms page is also looked for at its usual paths
When the homepage links to no terms page, the check SHALL use a terms link on the site's Jobs page,
and else try `/terms`, `/terms-of-service`, `/tos`, `/legal` and `/terms-and-conditions` in turn,
stopping at the first that is a terms page (a heading that says so and a page of text, never a page
that answers every address alike). The terms SHALL be read and the verdict decided exactly as before.

#### Scenario: A usual path
- **WHEN** the homepage has no terms link and `/terms-of-service` is a page titled "Terms of Service"
- **THEN** its sentences are kept and `/tos` is not asked for

#### Scenario: A site that answers every path with its front page
- **WHEN** every path returns the same page with no terms heading
- **THEN** no terms page is found and the report says so

### Requirement: Candidates are checked again once under new rules
A candidate checked under older rules SHALL be checked once more, oldest check first, after the new and
never-checked candidates, within the day's checks (`APPLICAMP_SOURCES__DISCOVERY_MAX_CHECKS`, 100) and
the AI caps. A candidate checked under the current rules SHALL be left alone. Terms SHALL be read by
the model again only when their text changed.

#### Scenario: More candidates than the day's checks
- **WHEN** three candidates were checked under old rules and the day allows two
- **THEN** the two with the oldest checks are checked, the third the next day, and none after that
