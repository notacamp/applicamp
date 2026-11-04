## MODIFIED Requirements

### Requirement: Import a profile for review
The app SHALL turn a LinkedIn data export ZIP, a PDF (LinkedIn profile or CV)
or pasted text into a draft profile that the person reviews, and SHALL save
nothing to the profile until they choose Replace or Merge. Uploaded files
SHALL NOT be stored. A model-read draft SHALL ask for every section (headline,
summary, positions, education, skills, languages, certifications, projects),
SHALL never be saved from an answer the model cut off or that is unusable, and
the review SHALL say in one plain line what was found in each part ("Found 6
jobs, 18 achievements, 9 skills, 2 degrees and 2 languages.") and list the
parts that came back empty (achievements included). A section the model
returned empty although the text has its heading (the headline and the About
text when it has an About heading) SHALL be asked for once more, in one call on
the text under those headings only (at most 8,000 characters), before it is
reported empty; if that call fails the draft SHALL be kept with those sections
empty, and if a spending limit refuses it the draft SHALL be kept and the
review SHALL say that the limit stopped it. A position's summary SHALL never be
a place: without a model, a summary that is a place, or begins with a place on
its own line, SHALL move to its location; a line is a place only when it has
commas, every part reads like a name and it ends with a country, or it is the
position's location or a part of it, a region, a country or a place with a way
of working, and never when it is longer than a place, holds a sentence or a
line break, a job title, or a list of tools. A summary that only says the title
or the company again SHALL go, bullet lines in a summary SHALL become its
highlights, and dates SHALL be `YYYY-MM` or `YYYY-MM-DD` (an end date that says
"Present" is none). A file picked as the other file kind (a PDF as the LinkedIn
export) SHALL be read as what it is.

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
- **WHEN** the model answers "Porto, Portugal", the line under a position's dates, as that position's summary, or as the first line of it
- **THEN** the draft has it as the position's location, the summary keeps only the description, and the review shows the place with the location

#### Scenario: Words that are not a place
- **WHEN** a summary says "Head of Engineering", "Rust, Kafka, PostgreSQL" or "Sold to retailers in Spain, Portugal, France"
- **THEN** it stays the summary

#### Scenario: Dates in words
- **WHEN** the PDF says "Feb 2019 - Nov 2020" and the model answers the dates as written
- **THEN** the draft's position starts `2019-02` and ends `2020-11`, and one that says "Present (1 year 4 months)" has no end date

#### Scenario: A section asked again
- **WHEN** the model returns no education and no languages for a PDF whose text has "Education" and "Languages" headings
- **THEN** both are asked once more on the text under those headings, and the draft holds what that answer gives; if it fails again they are listed as not found

#### Scenario: A limit stops the second read
- **WHEN** the day's AI limit is reached before the empty sections are asked again
- **THEN** the draft from the first two reads is kept, and the review says the limit stopped reading the missing parts again

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
background. Merging SHALL match positions one to one, by company and start
first and by company and title only when no position has that start, and
SHALL repair a position the profile and the draft both have: its summary SHALL
change only when the draft has a real paragraph for it (an empty summary takes
it; a summary that is only that position's place gives way to it, the place
moving to an empty location; a summary SHALL never be replaced when the
location holds a different place), an empty location, end date or start SHALL
be filled, and the draft's highlights SHALL be added once each; everything else
the person wrote or kept SHALL stay.

#### Scenario: Merge keeps edits
- **WHEN** they merge a new import into a profile they edited by hand
- **THEN** their edited values stay and only new positions, schools and skills are added, and the positions they share gain only what they lacked

#### Scenario: Merge repairs a place summary
- **WHEN** a position's summary is "Coimbra, Coimbra, Portugal", its location is empty, and a new import of the same position has a paragraph, an end date and three highlights
- **THEN** after Merge the position's location is "Coimbra, Coimbra, Portugal", its summary is the paragraph, its end date and highlights are there, and the facts kept on it are unchanged

#### Scenario: A summary someone wrote
- **WHEN** a position's summary is "Head of Engineering", or a place other than its location, or the new import has no paragraph for it
- **THEN** after Merge the summary is as it was

#### Scenario: Two stints, one title
- **WHEN** the profile has the current stint as Consultant at Example Org and the new import has it and an earlier one with the same title
- **THEN** the current stint keeps no end date and gains only its own highlights, and the earlier one is added as a position of its own

#### Scenario: A background
- **WHEN** the person saves "6 years in events and restaurants" from Tell me about you
- **THEN** it shows on the profile as their background and can be edited in place like the summary
