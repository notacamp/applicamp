## ADDED Requirements

### Requirement: Search reads approved boards by your top skills
For a person whose profile lists skills, a search SHALL read each approved board that can filter by
skill with one extra query per top skill (the first 3 skills, in the person's order), in addition
to the board's plain read, merge and deduplicate the answers, and rank jobs that name a top skill
higher before scoring. It SHALL stay within each board's polling limits and terms, add no model
call, and leave the scorer's caps unchanged. With no skills, or on a board with no skill filter,
it SHALL read as before.

#### Scenario: A Ruby on Rails developer
- **WHEN** the profile's first skills are Ruby on Rails, React and PostgreSQL and a search runs
- **THEN** RemoteOK is read for the tags `ruby`, `react` and `postgresql` as well as plainly, Jobicy for the keywords `ruby-on-rails`, `react` and `postgresql`, the jobs are merged without duplicates, and the ones naming those skills rank first

#### Scenario: A board with no skill filter
- **WHEN** a search runs and Arbeitnow is approved
- **THEN** Arbeitnow is read once, plainly

#### Scenario: A board's fetch limit
- **WHEN** a board allows 24 fetches a day and three stack queries are added
- **THEN** each of its four addresses is reused for 4 hours, so it is fetched at most 24 times a day

#### Scenario: No skills
- **WHEN** the profile lists no skills
- **THEN** every board is read exactly as before
