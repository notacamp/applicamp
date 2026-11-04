## ADDED Requirements

### Requirement: A LinkedIn check-up from the person's own file, with one model call
The app SHALL offer a page, "Your LinkedIn", that reads the person's LinkedIn profile from a file
they upload (the data export ZIP without a model, or the profile saved as PDF as the import reads
one), SHALL NOT store the file, and SHALL write a review with one further model call shown the
profile with its kept facts, the goals and the LinkedIn profile as read, never a name or a contact
detail. It SHALL NOT fetch from or write to LinkedIn. Check-ups SHALL be capped at 10 a day.

#### Scenario: The export ZIP
- **WHEN** a person with a profile and goals uploads their LinkedIn data export ZIP
- **THEN** after one model call the review shows a short overall read, one finding per part (headline, About, each position, skills, education, languages, certifications, projects) marked Good, Could be better or Missing with one sentence why, and proposals (a headline, an About text, a position's description, skills to add) each citing the profile sources, the kept facts or the LinkedIn text and saying what it does for the goals

#### Scenario: Nothing private is sent
- **WHEN** a check-up is made
- **THEN** the model's message holds no name, email, phone number, address or link of the person, and the stored LinkedIn profile holds none either

### Requirement: A proposal may say only what its sources say
Every proposal SHALL cite its sources and, with no model, every claim in its text SHALL be in them
(the claim check CVs and letters use); a buzzword (passionate, results-driven, guru, expert, go-to,
best and their equivalents in pt, es and fr) SHALL be flagged unless a cited source says it. A
proposal that cites nothing SHALL pass only if it claims nothing. Whatever fails SHALL stay on the
page with its flags and SHALL NOT be kept until edited to pass or dropped; the app drops nothing.

#### Scenario: A tool the profile never mentions
- **WHEN** a proposed headline says "Kubernetes" and no cited source says it
- **THEN** the proposal is shown with the flag naming Kubernetes, Keep is off, and editing it to say only what the sources say is checked again against the same sources and clears the flag

#### Scenario: Praise nobody wrote
- **WHEN** a proposed About text calls the person "passionate" and no cited source says it
- **THEN** the proposal is flagged as a buzzword and cannot be kept as it is

### Requirement: Keep, Drop, Done and Start over, all reversible
Each proposal SHALL offer Keep and Drop; a kept proposal SHALL appear under "To change on LinkedIn"
with its text to copy and Done, allowed only on a kept proposal. Every decision SHALL be reversible.
Start over SHALL delete the review, Check again SHALL replace it with a new file's, and a review SHALL
be deleted 30 days after its last touch. Nothing SHALL be written to the profile or the goals.

#### Scenario: Kept, copied, done
- **WHEN** someone keeps the proposed headline, copies it and marks it done
- **THEN** it shows under To change on LinkedIn as done, the profile is unchanged, and taking Done back returns it to kept

#### Scenario: The profile changed since
- **WHEN** the profile's version is no longer the one the review was made against
- **THEN** the page says the profile changed since and offers Check again, without blocking anything: every proposal still works

#### Scenario: Without a profile, or without a model
- **WHEN** someone without a profile opens Your LinkedIn, or this Applicamp has no `complete` model
- **THEN** it says to add the profile first, or that the check-up needs an AI model that is not set up, and nothing can be uploaded

#### Scenario: Another account
- **WHEN** another account asks for this person's review or one of its proposals
- **THEN** it is not found
