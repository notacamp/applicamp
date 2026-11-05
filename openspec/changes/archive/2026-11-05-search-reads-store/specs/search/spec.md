## ADDED Requirements

### Requirement: A search draws its jobs from the job store
A search SHALL take its candidates from the job store: the open jobs of the person's On job sites and
of every approved company page (the companies the person watches included), and SHALL NOT read those
sites or pages live. It SHALL read live only the person's own queries (their wanted roles and top
skills) on the sites that search by keyword, because the store reads only default feeds, and merge and
deduplicate those jobs with the store's. A nightly search SHALL take only the jobs the store first saw
since that person's last search. With the store turned off a search SHALL read the sites live.

#### Scenario: A search does not read the sites
- **WHEN** a site was read into the store and a person searches
- **THEN** the search finds the site's jobs without a request to the site

#### Scenario: Company pages
- **WHEN** thousands of company pages are approved and a person searches
- **THEN** their jobs are candidates, through the cheap preselect

#### Scenario: Only new jobs at night
- **WHEN** a nightly search runs and the person's last search was this afternoon
- **THEN** it takes only the jobs the store first saw since that search

#### Scenario: A night with new jobs always runs
- **WHEN** the store first saw jobs since a person's last search, they pass the person's hard limits and have no quick look or full read
- **THEN** the nightly job starts their search

### Requirement: A cheap preselect limits the quick looks
Before any model call, a search SHALL rank the jobs without a look from the store with Postgres full
text (the title, company and tags against the words of the person's wanted roles and top skills) plus
how new they are, and SHALL give a quick look only to the top `QUICK_LOOK_PER_SEARCH` (400). The others
SHALL be counted as "Not among the closest to your roles today", so the reasons still add up to the jobs
read, and be considered by a later search. It SHALL record the cost it expects before its first call, and
the estimate SHALL count the store's jobs without a score, at most that limit.

#### Scenario: A store of 40,000 jobs
- **WHEN** a nightly search finds 1,500 new jobs without a look
- **THEN** 400 get a quick look (16 calls), the best 100 are read in full (10 calls), and 1,100 are counted as not among the closest

#### Scenario: Looked at before
- **WHEN** a job had a quick look in an earlier search
- **THEN** it takes no slot of the 400 and competes again for a full read

## MODIFIED Requirements

### Requirement: Search reads approved job boards and credits them
A search SHALL use only job boards the owner approved, honour each board's
terms (polling limits, attribution, unchanged apply links, expiry), merge the
same job seen on several boards into one opportunity, and show every source
with a link to the original posting.

#### Scenario: The same job on two boards
- **WHEN** a job with the same apply link is in the store from Himalayas and RemoteOK
- **THEN** one opportunity is added, showing both boards as sources with links

#### Scenario: A board's polling limit
- **WHEN** a search runs twice within Remotive's allowed interval
- **THEN** Remotive is not requested by either search, and the store's jobs are used

### Requirement: The sites a person has On are what a search will read
A search SHALL draw from the job store only the approved job sites the person has On, after setting
the app's defaults from their current goals (`ensure_defaults`, then `on_sites`), together with every
approved company page. A site they turned Off SHALL contribute no job to their search, and neither
live queries nor the store's jobs of that site are used.

#### Scenario: Reading the person's sites
- **WHEN** a person turned one approved site Off and the app turned two On from their goals
- **THEN** a search for them uses the store's jobs of those two sites, and not the one

#### Scenario: A site turned Off
- **WHEN** a person turned a site Off
- **THEN** none of its jobs is a candidate of their search, though other people's searches still use them
