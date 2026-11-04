# companies Specification

## Purpose
Companies whose jobs keep fitting a person: suggested from their own matches with reasons traced to the
postings they come from, watched only after a yes, their own job board found without a model (a posting
link, the company's site within robots.txt, Greenhouse, Lever, Ashby or JobPosting data) or pasted, and read
every day by the search for new jobs.

## Requirements

### Requirement: A company is suggested from the person's own matches, without a model
A company SHALL be suggested when, in the last 8 weeks, the person has at least 2 matches from it, or 1 they shortlisted, and its
website is on record. A match SHALL be a job a search found, not hidden by a requirement and not set aside. A company SHALL NOT be
suggested when it is LinkedIn, Indeed or Glassdoor, when its site is a job board, a network or a hiring platform, when the person
already watches it, stopped watching it or said they are not interested. Suggestions SHALL be worked out with no model, when a
search ends and when the Companies page is opened and they are over an hour old.

#### Scenario: Two matches in eight weeks
- **WHEN** a company's site is on record and the person has two matches from it found within 8 weeks
- **THEN** it is suggested

#### Scenario: One shortlisted job
- **WHEN** the person shortlisted one job of a company and has no other match from it
- **THEN** it is suggested, and its first reason says they shortlisted a job

#### Scenario: Too long ago
- **WHEN** the second of two matches was found 9 weeks ago
- **THEN** the company is not suggested

#### Scenario: No site, or a network
- **WHEN** a company has no website on record, or its site is linkedin.com, indeed.com, glassdoor.com or a hiring platform
- **THEN** it is never suggested

#### Scenario: Not interested is for good
- **WHEN** the person says "Not interested" and their matches are worked out again later
- **THEN** the company is not suggested again

### Requirement: Every reason is traced to where it comes from
Each reason of a suggestion SHALL be worked out from the matched postings' own fields (the count and weeks of matches, the person's
skills the postings name, how many are remote, the places and the pay they state) or be a fact already read on the company's own site for
a CV, and SHALL cite the postings or facts it comes from, which the person can see with Why?. A reason SHALL NOT be written by a model
or say what no posting or fact says.

#### Scenario: A skill no posting names
- **WHEN** the person lists Kafka and none of the company's postings names it
- **THEN** no reason mentions Kafka

#### Scenario: Why?
- **WHEN** the person asks why a company is suggested
- **THEN** each reason shows the postings (with how well they fit) or the fact's company that it comes from

### Requirement: Nothing is watched without a yes
A company SHALL be watched only after the person presses Watch on a suggestion or pastes its careers page. The person SHALL be able to stop
watching it at any time, after which its job board is not read. At most 20 companies SHALL be watched at once; past that the app SHALL
refuse with a message saying the limit and how to make room.

#### Scenario: The 21st company
- **WHEN** the person watches 20 companies and tries to watch another
- **THEN** it is refused with "You can watch up to 20 companies. Stop watching one to add another." and nothing is added

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

### Requirement: A company can be watched by pasting its careers page
The person SHALL be able to paste a careers page, or a link to a Greenhouse, Lever or Ashby board, to watch a company themselves. The
company SHALL be added with origin `pasted` and its board looked for the same way; a link to a board SHALL NOT be fetched, only its API.
A LinkedIn, Indeed or Glassdoor address SHALL be refused.

#### Scenario: A pasted careers page
- **WHEN** the person pastes a page that links a Greenhouse board
- **THEN** the company appears under Watching named as its page names it, with its board found

### Requirement: The Companies page
`/companies` SHALL show Suggested and Watching tabs with their counts, a card per suggestion with its reasons, Why?, Watch and Not
interested, the watched companies with their board, open jobs, last check (or why it failed) and Stop watching, a box to watch a
company yourself, and "How it works & cost", in the person's language, with no sideways scroll at 375px.

#### Scenario: A failed check
- **WHEN** the last read of a watched board failed with a 503
- **THEN** the company stays watched and its row says "Last check failed: the board answered 503"

### Requirement: A watched company reuses the registry's company page
When a watched company's board is a hiring system's company page, the company SHALL be linked to
that registry row instead of keeping a copy, the row SHALL record what the watch read, and
watching SHALL work as before whether or not searches read company pages.

#### Scenario: The page is already there
- **WHEN** the person watches a company whose Greenhouse page the registry holds
- **THEN** the company is linked to it and no second page is added
