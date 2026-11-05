## ADDED Requirements

### Requirement: Approved sites are read on their own schedule into a job store
The app SHALL read every approved job board, and every company page whose hiring system is
approved and whose robots rules allow reading, on its own schedule, and keep the jobs they list in
one store with, for each job, the site it came from, its reference there, its title, company, links,
place, how it is worked, pay, dates and description (cut at a limit), when the app first and last
saw it and when it closed. Only approved rows SHALL be read; a paused, declined or waiting site, a
company page whose system waits, and a site `APPLICAMP_SOURCES__ENABLED` leaves out SHALL NOT. The
app SHALL call no model while reading, and SHALL feed every read's apply links to the registration
of company pages, as a search's read does. A search SHALL still read boards live until it reads the
store.

#### Scenario: A board approved by the app
- **WHEN** a job board is approved
- **THEN** the next reading reads it without anyone searching, and its jobs are in the store

#### Scenario: A waiting or paused site
- **WHEN** a board waits for a decision, or an approved board is paused
- **THEN** it is not read for the store, and its stored jobs are not offered to a search

#### Scenario: A company page
- **WHEN** a company page and its hiring system are approved
- **THEN** its jobs are read through the system's reader into the store, once a day at most

### Requirement: A site's own limits come first
The app SHALL read a site no oftener than its polling interval (counting searches that read it
live), its terms' fetches a day, the `Retry-After` it gave and the instance's least interval between
two reads; a company page at most once a day. It SHALL keep to one request at a time on a host, at
the instance's pace between two requests to it, and start at most a set number of reads a minute
over all hosts. A site that answered 429 or 503 SHALL be left alone for as long as it said, an hour
if it said nothing.

#### Scenario: Fetches a day
- **WHEN** a board's terms allow two fetches a day
- **THEN** it is read every twelve hours at the soonest

#### Scenario: A search read it lately
- **WHEN** a search read a board live five minutes ago and its polling interval is fifteen minutes
- **THEN** the store does not read it until the interval has passed

#### Scenario: Asked to slow down
- **WHEN** a board answers 429 with `Retry-After: 7200`
- **THEN** it is not read again for two hours, and its line in the log of reads says it was rate limited

### Requirement: Only new or changed jobs are kept, and a site is asked whether it changed
The app SHALL write a job only when it is new or changed since the last read, and touch the ones it
sees again. It SHALL send the `ETag` and `Last-Modified` a site gave as a conditional request
where the reader's answer is one list, and when the site answers that nothing changed SHALL keep
every job and parse nothing.

#### Scenario: Nothing changed
- **WHEN** a site answers 304 to a conditional request
- **THEN** its jobs stay open as they are, the read is logged as not modified, and the site is seen again

#### Scenario: One job changed
- **WHEN** a read lists a job with a new title, a job it had, and a job it did not
- **THEN** one job is rewritten, one is added, and the other is only seen again

### Requirement: Jobs a site no longer lists are closed, then deleted
When a read lists everything a site has, the app SHALL close the stored jobs it no longer lists, and
reopen a job the site lists again. A read stopped by a page limit, a read through a sitemap, and a
read that would close most of a site's jobs at once SHALL close nothing. The app SHALL delete a
closed job after a number of days (14 by default), drop a job past its expiry (for a site whose
terms say to drop expired jobs) and delete a job no read has seen for 30 days, and SHALL keep a
limited number of jobs per site and in all, the newest.

#### Scenario: A job the board dropped
- **WHEN** a full read no longer lists a job
- **THEN** it is closed, not offered to a search, and deleted fourteen days later

#### Scenario: A partial answer
- **WHEN** a read stops at its page limit with more pages announced
- **THEN** the jobs it did not list stay open

#### Scenario: An expired job
- **WHEN** a board whose terms say to drop expired jobs lists a job past its expiry
- **THEN** the job is not stored

### Requirement: A quiet site is read less often
The app SHALL read a site less often as its reads bring nothing new (twice as far apart after three
reads in a row with nothing new, then four, eight and sixteen times, up to a day, a week for a
company page), and SHALL return to its own interval when a read brings something, never reading
oftener than its limits allow.

#### Scenario: Nothing new
- **WHEN** a board's third read in a row brings nothing new
- **THEN** its next read is twice as far away as before

### Requirement: Searches can read the store, and the owner sees how it goes
The module SHALL offer a read of the open jobs of approved sites filtered on posted since, work
mode, regions and the words of someone's roles and skills, with a limit, giving jobs as the readers
give them. `GET /api/v1/boards/store` SHALL tell the board owners how many jobs are stored, how many
are new in the last 7 days, how many sites were read today and how many reads failed, and SHALL
refuse anyone else.

#### Scenario: The owner's totals
- **WHEN** a board owner asks for the store's totals after a reading
- **THEN** the answer counts the jobs stored, those new in 7 days, the sites read today and the failures

#### Scenario: Someone else
- **WHEN** an account that does not manage the boards asks
- **THEN** it is refused
