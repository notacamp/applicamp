## MODIFIED Requirements

### Requirement: See what was scanned
The Searches tab of the Job sites page SHALL show each search with four
numbers first: jobs read, read in full, good matches and its cost. Under the
latest search it SHALL show where the matches came from, by site, and why the
other jobs were left out, in plain words with counts that add up to the jobs
read, and how many of the jobs read in full do not say where they hire and
were kept with a note. What the search did on each site (jobs read, quick
look, read in full, good matches, cost, and why a site was skipped, failed or
read in part) SHALL be behind "Show by site". Earlier searches SHALL be one
row each with their four numbers and Details; a discovery run SHALL be one
row with what it checked under Details. Search now SHALL be a plain button
with the estimated cost on the line under it and no keyboard shortcut shown.

#### Scenario: After the nightly search
- **WHEN** the nightly search has run
- **THEN** the Searches tab shows it first with the jobs read, read in full, good matches and cost, and under "Show by site" each site's counts and its cost, a site skipped for its polling limit saying so

#### Scenario: The counts add up
- **WHEN** a search read 1,112 jobs and found 6 good matches
- **THEN** the reasons under "Why jobs were left out" add up to 1,106

#### Scenario: A site read in part
- **WHEN** a site's third page did not answer during a search
- **THEN** its line under "Show by site" says so, and its first two pages count as read
