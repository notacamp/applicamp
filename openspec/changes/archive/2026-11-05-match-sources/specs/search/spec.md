## ADDED Requirements

### Requirement: Each good match is credited to one source
A search SHALL credit every good match it adds to exactly one source, either a job site or a company's
own hiring page (a company page, or a watched company's job board), so that the matches credited to
sources add up to the good matches. When the same job came from several places, the copies SHALL merge
into one match that keeps every place it came from, and it SHALL be credited to the company page when
one is among them, else to the site that listed it first. A match the person's pipeline already had
SHALL NOT be credited, nor counted as a good match.

#### Scenario: A site and a company page
- **WHEN** a job is on a job site and on its company's page, and it is a good match
- **THEN** it is added once, credited to the company page, and the site's link is kept on it

#### Scenario: Two sites
- **WHEN** a job is on Himalayas and on Remote OK and no company page lists it
- **THEN** it is credited to Himalayas, which listed it first, and to no other source

#### Scenario: The totals add up
- **WHEN** a search adds 16 good matches from sites and company pages
- **THEN** the matches credited to its sources add up to 16
