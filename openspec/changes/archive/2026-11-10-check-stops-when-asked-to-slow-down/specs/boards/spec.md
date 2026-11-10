## ADDED Requirements

### Requirement: A site that asks the check to slow down is not asked again in that check
When a site answers a page of a check with 429 or 503 (or the reader's own back-off for it), the check SHALL
send that host no more requests: its later pages read as nothing, SHALL NOT be asked, SHALL NOT count against
the request budget, and the log SHALL say it once per host. Other hosts, such as a feed on another domain, SHALL
still be read. A candidate whose check was stopped this way and that found no list of jobs SHALL wait with the
code `asked_to_slow_down`, worded on the admin page in every language ("The site asked the app to slow down, so
the check stopped."); this reason SHALL come before the request limit. When a list of jobs was found, the usual
reasons apply. The check SHALL NOT be retried on its own: the candidate is checked again only when the rules
change or the owner presses "Check again".

#### Scenario: robots.txt says slow down
- **WHEN** a site's robots.txt answers 429
- **THEN** no later request is made to that host, and the candidate waits with `asked_to_slow_down`

#### Scenario: A 503 counts too
- **WHEN** a site's homepage answers 503
- **THEN** the rest of the check does not ask that host

#### Scenario: Another host
- **WHEN** a site said slow down and its homepage linked a feed on another host
- **THEN** the feed is still read

#### Scenario: A list was found
- **WHEN** a list of jobs was found before a host said slow down
- **THEN** the candidate's reason is the usual one, not `asked_to_slow_down`

#### Scenario: The budget
- **WHEN** a host said slow down
- **THEN** its skipped pages do not spend the request budget
