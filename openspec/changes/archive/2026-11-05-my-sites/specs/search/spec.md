## ADDED Requirements

### Requirement: The sites a person has On are what a search will read
The module SHALL offer, for searches, the approved job sites a person has On, after setting the
app's defaults from their current goals (`ensure_defaults`, `on_sites`). Until searches read the job
store (#89, second part, which applies this), a search SHALL still read every approved site.

#### Scenario: Reading the person's sites
- **WHEN** a person turned one approved site Off and the app turned two On from their goals
- **THEN** the sites offered to a search for them are the two, and not the one

#### Scenario: Searches today
- **WHEN** a search runs before the second part of #89 is built
- **THEN** it reads every approved site as before, whatever is On or Off
