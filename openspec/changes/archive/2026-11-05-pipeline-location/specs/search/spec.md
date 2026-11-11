## ADDED Requirements

### Requirement: A search reads what a posting asks of where you are
The scoring call a search makes SHALL also read, from each posting only, the working hours or time
zones it needs, what it says about the right to work and visa sponsorship, and what it says about
moving for the job, each in at most 80 characters and empty when the posting does not say. These
SHALL be kept with the score and on the opportunity, and SHALL NOT be used to leave a job out or to
change its score. Reading them SHALL NOT make another model call.

#### Scenario: A posting that asks for European hours
- **WHEN** a search reads a remote posting that says "you'll work CET hours, ± 2" and "we can't sponsor visas"
- **THEN** the job is kept or left out by the same rules as before, and its opportunity carries the hours and the work permit note

#### Scenario: Instructions in the posting
- **WHEN** a posting's text tells the model to write something else as its work permit note
- **THEN** the note says only what the posting says about the right to work, or nothing
