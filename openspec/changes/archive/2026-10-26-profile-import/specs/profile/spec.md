## ADDED Requirements

### Requirement: Import a profile for review
The app SHALL turn a LinkedIn data export ZIP, a PDF (LinkedIn profile or CV)
or pasted text into a draft profile that the person reviews, and SHALL save
nothing to the profile until they choose Replace or Merge. Uploaded files
SHALL NOT be stored.

#### Scenario: A LinkedIn export
- **WHEN** a signed-in person uploads their LinkedIn export ZIP
- **THEN** a draft holds their positions, education, skills and languages from its CSVs, without any model call, and their profile is unchanged until they apply it

#### Scenario: A CV as PDF
- **WHEN** they upload a CV PDF with text
- **THEN** a draft holds the structured CV and says how many details were left out because they did not appear in the file

#### Scenario: A scanned PDF
- **WHEN** the PDF has no extractable text
- **THEN** the import fails with a message saying so and nothing is created

#### Scenario: A hostile archive
- **WHEN** a ZIP holds a path outside the archive, too many entries or more data than the limits once uncompressed
- **THEN** the import is refused before anything is extracted

### Requirement: Nothing invented
A draft structured by a model SHALL contain only values that appear in the
source text; any other value SHALL be dropped and counted.

#### Scenario: A made-up employer
- **WHEN** the model returns a company name that is not in the CV text
- **THEN** that position is not in the draft and the dropped count includes it

### Requirement: Edit the profile
The person SHALL be able to see and edit every part of their profile, and an
import SHALL be able to merge into it without losing what they edited.

#### Scenario: Merge keeps edits
- **WHEN** they merge a new import into a profile they edited by hand
- **THEN** their edited values stay and only new positions, schools and skills are added
