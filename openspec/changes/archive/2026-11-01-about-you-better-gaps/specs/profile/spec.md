## MODIFIED Requirements

### Requirement: Tell me about you asks about the gaps, one at a time
After "Anything else", Tell me about you SHALL ask questions about what the app does not know or only assumes, one at a time, each skippable, and the person SHALL be able to stop at any point with "That's enough for now". What is asked SHALL be decided in code from the profile, the kept facts, the confirmed and wrong assumptions, the goals and the wizard's state, and never by a model. The gaps SHALL be ranked: assumptions the person left for later (and, after "Not really", those they called wrong), then the most recent role (what the person did there first, when the profile says nothing of that role, then its results without a number), then empty skills, then no languages, then the rest: a lead or manager title without a team size, the other recent roles the same way as the first, technologies the roles name that are not skills, a short skills list (fewer than six) that the roles name nothing more about, a skill the person told that none of their roles mention, an empty summary, headline or background, no education, goals not answered, and each area of each recent role nothing has been kept about. An answer about languages SHALL be saved as one language per change with its fluency when given, and an answer about education as one degree, course or bootcamp per change (or "Self-taught"). What the person has told so far SHALL close the gap it answers. The company's context SHALL never be asked. Questions SHALL stop when no gap is left, after 12 (8 after "Yes, mostly", 16 after "Not really"), or when the person says so. A skipped question SHALL not come back for the same profile version. Each question SHALL show how many have been asked of about how many, and the profile's strength with "→ N% if you answer", computed from the gap's weight in the strength.

#### Scenario: An assumption left for later is asked first
- **WHEN** the person chose Ask me later on "I assume you shipped pipelines" and continues
- **THEN** the first question is about it, saying what was assumed and why

#### Scenario: A role without a result
- **WHEN** the most recent role states no result with a number
- **THEN** a question about what changed because of the person's work comes next, tagged I don't know, with the strength it would add, and a number is optional

#### Scenario: A lead title without a team size
- **WHEN** a position is titled Lead Engineer and nothing is kept about leading people
- **THEN** a question about the team size is asked, with buckets to pick from

#### Scenario: Skills the roles name
- **WHEN** the roles mention Kafka and the skills do not
- **THEN** the question says "Your roles mention Kafka, but it isn't a skill" and offers Kafka as a choice

#### Scenario: A claim no role backs
- **WHEN** the person wrote that they use Elixir and none of their roles mention it
- **THEN** a question tagged You told me asks where they used it

#### Scenario: Context is not asked
- **WHEN** the company's website was read, or the portrait assumed what the company does
- **THEN** no question asks what the company sells; at most the assumption is asked to be confirmed

#### Scenario: Skip and enough
- **WHEN** the person skips a question
- **THEN** it is not asked again for this profile version, no model is called, and the next comes; and **WHEN** they choose That's enough for now, no more is asked and they go to the review

#### Scenario: The list runs out
- **WHEN** no gap is left, or 12 questions have been answered or skipped
- **THEN** the person goes to the review

#### Scenario: Not really
- **WHEN** the person answered "Not really" to "Is this accurate?"
- **THEN** up to 16 questions are asked, and the assumptions they marked wrong are asked about too

#### Scenario: A role nobody described
- **WHEN** the most recent role has only a title and dates
- **THEN** the first question asks what the person did there, and the question about what changed because of their work comes after it

#### Scenario: No languages
- **WHEN** the profile lists no languages and the goals say they work in English and Portuguese
- **THEN** a question asks which languages they speak and how well, its reason naming English and Portuguese, and the answer "Portuguese (native)" becomes the language Portuguese with fluency native

#### Scenario: No education
- **WHEN** the profile lists no education
- **THEN** after the summary and headline, a question asks about a degree, course or bootcamp, and "Self-taught" is an answer

#### Scenario: A short skills list
- **WHEN** the profile lists four skills and the roles name no other technology
- **THEN** a question names the four and asks what else the person works with
