## ADDED Requirements

### Requirement: The check follows a site's redirects within the site
During a check, every page the app reads from a site (robots.txt, the homepage, the terms page, feeds,
sitemaps, the API paths it probes) SHALL follow a redirect to another address of the same site (the same host,
`www.` or not), at most 3 hops. Each hop SHALL count as one request of the check's budget, pass the same
public-address check and, once the site's robots rules are known, be allowed by them; a hop the rules disallow
SHALL NOT be read. A redirect to another site SHALL NOT be followed. The check SHALL carry on from the address
the homepage ended up at: links, the terms link and the places looked for a list of jobs, with the robots rules
of that address's host. An address the check proposes or applies in a config SHALL be the final address of the
feed, API or sitemap, never the one that redirected; an owner's edited config SHALL NOT be rewritten. When the
homepage redirects to another site, the report SHALL keep that site's host (`moved_to`) and the waiting reason
SHALL stay as it is. Reads for the job store and for searches SHALL still never follow a redirect. The log of a
page that could not be read SHALL name its host, path and status, never its query. The check's rules version
goes up, so every candidate is checked again once.

#### Scenario: The homepage moves
- **WHEN** a site's homepage redirects to a path (or to `www.`) of the same site, which links to a feed
- **THEN** the check reads the page it ended up at, finds the feed through its links, and reads the terms and
  the other paths from there

#### Scenario: A feed address moves
- **WHEN** `/feed` answers 301 to `/feed/` and that is a feed
- **THEN** the feed is read, and the proposed config names `/feed/`

#### Scenario: A fourth hop
- **WHEN** an address redirects three times and the last one redirects again
- **THEN** the first three are followed and the fourth is not asked

#### Scenario: Another site
- **WHEN** a site's homepage redirects to another site
- **THEN** the other site is not asked, and the report's `moved_to` holds its host

#### Scenario: A hop robots.txt disallows
- **WHEN** an address redirects to a path the site's robots rules disallow
- **THEN** that path is not asked

#### Scenario: Hops use the budget
- **WHEN** an address redirects three times and the check's budget is 5 requests
- **THEN** the hops count as requests, and the check stops when the 5 are used

#### Scenario: Other reads
- **WHEN** a job store read or a search's read gets a redirect
- **THEN** it is a failed read and the target is not asked
