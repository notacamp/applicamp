## MODIFIED Requirements

### Requirement: Import a profile for review
The app SHALL turn a LinkedIn data export ZIP, a PDF (LinkedIn profile or CV)
or pasted text into a draft profile that the person reviews, and SHALL save
nothing to the profile until they choose Replace or Merge. Uploaded files
SHALL NOT be stored. A model-read draft SHALL ask for every section (headline,
summary, positions, education, skills, languages, certifications, projects),
SHALL never be saved from an answer the model cut off or that is unusable, and
the review SHALL say in one plain line what was found in each part ("Found 13
jobs, 31 achievements, 14 skills, 1 degree and 3 languages.") and list the
sections that came back empty. A section the model returned empty although
the text has its heading SHALL be asked for once more, on the text under that
heading only, before it is reported empty. A position's summary SHALL never be
a place: without a model, a summary that is the position's location or a short
line that reads like "City, Region, Country" and says no verb SHALL move to its
location, a summary that only says the title or the company again SHALL go,
bullet lines in a summary SHALL become its highlights, and dates SHALL be
`YYYY-MM` (an end date that says "Present" is none). A file picked as the other
file kind (a PDF as the LinkedIn export) SHALL be read as what it is.

#### Scenario: A LinkedIn export
- **WHEN** a signed-in person uploads their LinkedIn export ZIP
- **THEN** a draft holds their positions, education, skills and languages from its CSVs, without any model call, and their profile is unchanged until they apply it

#### Scenario: A CV as PDF
- **WHEN** they upload a CV PDF with text
- **THEN** a draft holds the structured CV and says how many details were left out because they did not appear in the file

#### Scenario: A LinkedIn profile as PDF
- **WHEN** they upload their LinkedIn profile saved as a two-column PDF with a headline, an About text, skills, education and languages
- **THEN** the draft holds the headline, the summary, the skills, the education and the languages as well as the positions, and the review says how many of each it found

#### Scenario: A place as a summary
- **WHEN** the model answers "Lisbon, Portugal", the line under a position's dates, as that position's summary
- **THEN** the draft has it as the position's location, the position has no summary, and the review shows it with the location

#### Scenario: Dates in words
- **WHEN** the PDF says "Jul 2023 - Apr 2024" and the model answers the dates as written
- **THEN** the draft's position starts `2023-07` and ends `2024-04`, and one that says "Present" has no end date

#### Scenario: A section asked again
- **WHEN** the model returns no education and no languages for a PDF whose text has "Education" and "Languages" headings
- **THEN** both are asked once more on the text under those headings, and the draft holds what that answer gives; if it fails again they are listed as not found

#### Scenario: A PDF picked as the export
- **WHEN** someone picks a PDF while the LinkedIn export is the chosen kind
- **THEN** it is read as a PDF

#### Scenario: An answer cut off
- **WHEN** the model stops for length or answers something that is not the expected JSON, even after one more try
- **THEN** the import fails with a message saying so and nothing is created

#### Scenario: Parts that came back empty
- **WHEN** a draft has no skills
- **THEN** the review says "No skills found in this file" and suggests adding them by hand or importing the LinkedIn data export

#### Scenario: A scanned PDF
- **WHEN** the PDF has no extractable text
- **THEN** the import fails with a message saying so and nothing is created

#### Scenario: A hostile archive
- **WHEN** a ZIP holds a path outside the archive, too many entries or more data than the limits once uncompressed
- **THEN** the import is refused before anything is extracted

### Requirement: Edit the profile
The person SHALL be able to see and edit every part of their profile,
including a background (where they come from: domains and earlier careers, in
their words), and an import SHALL be able to merge into it without losing what
they edited. The import and the model that structures it SHALL NOT write the
background. Merging SHALL repair a position the profile and the draft both
have: a summary that is only a place SHALL move to the location and the
draft's summary SHALL take its place (an earlier import's mistake, not an
edit), an empty location, end date or summary SHALL be filled, and the draft's
highlights SHALL be added once each; everything else the person wrote or kept
SHALL stay.

#### Scenario: Merge keeps edits
- **WHEN** they merge a new import into a profile they edited by hand
- **THEN** their edited values stay and only new positions, schools and skills are added, and the positions they share gain only what they lacked

#### Scenario: Merge repairs a place summary
- **WHEN** a position's summary is "Lisboa, Lisboa, Portugal" and a new import of the same position has a paragraph, an end date and three highlights
- **THEN** after Merge the position's location is "Lisboa, Lisboa, Portugal", its summary is the paragraph, its end date and highlights are there, and the facts kept on it are unchanged

#### Scenario: A background
- **WHEN** the person saves "6 years in events and restaurants" from Tell me about you
- **THEN** it shows on the profile as their background and can be edited in place like the summary
