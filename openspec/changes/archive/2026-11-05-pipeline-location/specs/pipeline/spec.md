## ADDED Requirements

### Requirement: Each opportunity says where it is and what it asks of where you are
The pipeline SHALL show, for every opportunity in the table, on the cards and on the board, where
the job is (its place and work mode, as the posting says) and, when its posting was checked, what it
asks of where the person is: whether it is open to where they live with the region it names, the
working hours or time zones it needs, what it says about the right to work and visa sponsorship,
and what it says about moving. The peek SHALL show the hours, the work permit and moving each on its
own line when the posting says them. What a posting does not say SHALL NOT be shown, and nothing
SHALL be guessed.

#### Scenario: A job in Berlin that needs an EU work permit
- **WHEN** someone opens the pipeline and a hybrid job in Berlin, open to the EU, asks for CET ± 2 hours and an EU work permit with no visa sponsorship
- **THEN** its row shows "Berlin, Germany · Hybrid" and "Open to where you live: EU · Hours: CET ± 2 hours · Work permit: EU work permit, no visa sponsorship"

#### Scenario: A posting that says none of it
- **WHEN** an opportunity was added by link, or found before its requirements were checked
- **THEN** its row shows only where it is, and the peek shows no Hours, Work permit or Moving line
