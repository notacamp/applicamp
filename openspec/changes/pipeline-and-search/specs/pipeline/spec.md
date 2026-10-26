## ADDED Requirements

### Requirement: One pipeline of opportunities
The app SHALL keep every opportunity a person has, found by a search or added
by link, in one pipeline with a stage (Found, Shortlisted, Applying, Applied,
Talking, Interview, Offer, Closed), viewable as a table and as a board, each
opening to its details, why it matches, its gaps, notes and the posting.

#### Scenario: Moving a job forward
- **WHEN** someone opens a Found opportunity and sets its stage to Shortlisted
- **THEN** it shows under Shortlisted on the board and in the table, with the change kept

#### Scenario: Adding a job by link
- **WHEN** someone pastes a Greenhouse posting link
- **THEN** the opportunity appears with its title, company and location read from Greenhouse, and that company's board is searched from then on

#### Scenario: A link to any other site
- **WHEN** someone pastes a link to a site that is not a supported job board
- **THEN** an opportunity is created with the link and they type its title and company; the server does not fetch that site
