## MODIFIED Requirements

### Requirement: See what was scanned
The Searches tab of the Job sites page SHALL show each search with four
numbers first: jobs read, read in full, good matches and its cost. Under the
latest search it SHALL show how many sites and company pages the job store gave
jobs from, where the matches came from (each match under one source,
a job site or a company's hiring page shown by the company's name, the five biggest
one by one and the rest as one line each for company pages and for sites, adding
up to the good matches), and why the
other jobs were left out, in plain words with counts that add up to the jobs
read (including the jobs not among the closest to the person's roles, which got
no quick look), and how many of the jobs read in full do not say where they hire and
were kept with a note. What the search did on each site (jobs read, quick
look, read in full, good matches, cost, and why a site was skipped, failed or
read in part) SHALL be behind "Show by site". Earlier searches SHALL be one
row each with their four numbers and Details; a discovery run SHALL be one
row with what it checked under Details. Search now SHALL be a plain button
with the estimated cost on the line under it and no keyboard shortcut shown.

#### Scenario: After the nightly search
- **WHEN** the nightly search has run
- **THEN** the Searches tab shows it first with the jobs read, read in full, good matches and cost, and under "Show by site" each site's counts and its cost, a site that failed saying so

#### Scenario: The counts add up
- **WHEN** a search read 1,112 jobs and found 6 good matches
- **THEN** the reasons under "Why jobs were left out" add up to 1,106

#### Scenario: Read from the store
- **WHEN** a search drew jobs from 4 sites and 212 company pages
- **THEN** the tab says it read from 4 sites and 212 company pages

#### Scenario: A site read in part
- **WHEN** a site's third page did not answer during a search
- **THEN** its line under "Show by site" says so, and its first two pages count as read

#### Scenario: Where the matches came from
- **WHEN** a search found 16 good matches, 9 credited to Himalayas, 5 to Wayfarer Co's page and 2 to Remote OK
- **THEN** the tab lists "Himalayas · 9", "Wayfarer Co’s hiring page · 5" and "Remote OK · 2", adding up to 16

#### Scenario: Many company pages
- **WHEN** twelve company pages gave one or two matches each
- **THEN** the list shows the five biggest sources and then "N company hiring pages" as one line with their total
