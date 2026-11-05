## ADDED Requirements

### Requirement: The Workable list adds every Workable company that is hiring, once a day
The app SHALL read Workable's documented list of every company's jobs
(`https://www.workable.com/boards/workable.xml`) at most once a day, as a stream: it SHALL parse
the list while it downloads and never hold its body in memory, SHALL check robots.txt for the
list's path before each download, SHALL send `If-None-Match` and `If-Modified-Since` from the last
complete download and treat `304` as unchanged, SHALL honour `429` and `503` with `Retry-After`,
and SHALL stop at a configured byte cap and time limit. A download that is cut or interrupted
SHALL keep the counts of what it processed, and SHALL NOT be the one the next request is
conditional on. The list SHALL be used only to grow the registry: no model SHALL be called, and the
jobs themselves SHALL NOT be stored, only how many jobs the list had for each company. Job links
SHALL be kept exactly as the list gives them. A company on the configured exclusion list (boards
Workable already integrates with, empty by default) SHALL never be added from the list.

#### Scenario: The first read
- **WHEN** the list is read for the first time
- **THEN** each company with a job posted in the last 60 days is counted with its job count, and a company with only old jobs is not added

#### Scenario: Not modified
- **WHEN** the server answers `304` to the conditional request
- **THEN** nothing is downloaded, the validators are kept, and the pages already added stay

#### Scenario: Cut by the byte cap
- **WHEN** the download reaches its byte cap
- **THEN** the companies counted so far are kept, the outcome is recorded as capped, and the next request is not conditional

#### Scenario: Asked to wait
- **WHEN** the server answers `429` with `Retry-After`
- **THEN** no download is made before that time (an hour at least, a day at most)

### Requirement: Listed companies become Workable company pages that the app approves with their system
For each company the list names with a job posted in the last 60 days, the app SHALL find its
address on Workable from one job's short link (a redirect to the company's page, one request,
robots rules respected, at most a configured number a day), and SHALL upsert a Workable company
page in batches, recording "the Workable list", when the list last had it and how many jobs. The
app SHALL approve such a page by itself when the Workable hiring system is approved, without a
sample read, and the row's report SHALL say that the list itself shows the company posts jobs.
While the Workable system waits for the owner, its listed pages SHALL wait with it; a declined or
paused system SHALL NOT be read.

#### Scenario: Workable is approved
- **WHEN** the Workable hiring system is approved and the list has a company with a recent job
- **THEN** its company page is added and approved by the app, with no sample read, and its report says so

#### Scenario: Workable waits for the owner
- **WHEN** the Workable system waits because its robots.txt says `ai-train=no`
- **THEN** the listed company pages are added as candidates and none is approved until the owner approves the system

#### Scenario: A company found before
- **WHEN** a company page was already registered through an apply link and the list names it
- **THEN** no second page is added, and it gains when the list last had it and its job count
