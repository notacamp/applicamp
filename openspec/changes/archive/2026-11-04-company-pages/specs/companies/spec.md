## MODIFIED Requirements

### Requirement: The job board is found without a model, respecting robots.txt
When a company is watched, the app SHALL use the Greenhouse, Lever, Ashby, Workable, Recruitee,
Teamtailor or Personio board a posting of the company already came from, else look on the company's
own site (its home page, the careers pages it links, and `/careers`, `/jobs`, `/join-us`,
`/work-with-us`) for a link, embed or token of one of those boards, or schema.org `JobPosting` data.
It SHALL read only pages the site's robots.txt allows, only through the existing guarded readers,
and never follow a redirect off the company's site. A board found SHALL be read once to confirm it
lists jobs, and the postings then listed SHALL be noted as seen so that only later postings count.
A board found SHALL be linked to its company page in the registry, which SHALL be added, found
through "your company watch", when it is not there yet.

#### Scenario: A Greenhouse board linked from the careers page
- **WHEN** the company's careers page links its Greenhouse board
- **THEN** the board is found, confirmed by reading it, and shown under Watching with its open jobs

#### Scenario: A Workable board
- **WHEN** a posting of the company says to apply at `https://apply.workable.com/acme/j/ABC123/`
- **THEN** the Workable board of `acme` is used, and read once to confirm it lists jobs

#### Scenario: JobPosting data
- **WHEN** the careers page holds schema.org JobPosting data and no board link
- **THEN** the page itself is the company's job board

#### Scenario: robots.txt forbids the careers page
- **WHEN** the site's robots.txt disallows `/careers`
- **THEN** that page is never requested

#### Scenario: Nothing found
- **WHEN** no board is found
- **THEN** the watch says "Couldn't find their job board" with a box to paste the careers page, which is looked at the same way

## ADDED Requirements

### Requirement: A watched company reuses the registry's company page
When a watched company's board is a hiring system's company page, the company SHALL be linked to
that registry row instead of keeping a copy, the row SHALL record what the watch read, and
watching SHALL work as before whether or not searches read company pages.

#### Scenario: The page is already there
- **WHEN** the person watches a company whose Greenhouse page the registry holds
- **THEN** the company is linked to it and no second page is added
