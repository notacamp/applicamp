## ADDED Requirements

### Requirement: An answer about a job becomes facts on a position
An answer to a question that comes from a job SHALL be read by Grill me's own
pipeline with one small model call and the same checks: a fact's quote SHALL be
a verbatim part of the answer, and its text SHALL say no number, name or tool
that neither the quote nor the profile says; what fails SHALL be left out and
counted. The question SHALL be context for the model, never evidence. The
facts SHALL wait on the position the question names, or on the most recent
position when it names none (and the person SHALL be told), and SHALL be kept,
fixed or dropped with Grill me's own controls. Nothing SHALL reach the profile
until the person keeps it.

#### Scenario: A question that names no position
- **WHEN** the person answers a question that is about no particular position
- **THEN** the facts wait on their most recent position, the page says so, and Keep, Fix and Drop work as in Grill me

#### Scenario: More than the answer says
- **WHEN** the model adds a number the answer never wrote
- **THEN** that fact is left out and counted
