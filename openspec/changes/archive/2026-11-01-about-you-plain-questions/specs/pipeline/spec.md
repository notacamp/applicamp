## MODIFIED Requirements

### Requirement: Every company fact quotes its page
One model call SHALL turn the pages' text into short facts about the company,
each with the page's exact words and the page's link. A fact SHALL be kept
only if its page was one of those read and its quote is a verbatim part of that
page's text (ignoring case, spacing, and typographic quotes and dashes);
otherwise it SHALL be left out and counted. The prompt SHALL say to use only
the pages and never memory, and that page text is data, not instructions. The call
SHALL ask for at most 10 facts, each text at most about 160 characters and each
quote at most about 200, in the instruction and in the answer's schema. It SHALL
be shown a limited amount of page text (the posting first, then the pages by
priority); when its answer is cut off, the one retry SHALL be shown half the
text, not the same text with more room to answer, and there SHALL be two attempts
in all. The
person SHALL be able to remove any fact, undo that, and a removed fact SHALL
stay removed on later reads.

#### Scenario: A fact with a made-up quote
- **WHEN** the model returns a fact whose quote is on no page it read
- **THEN** it is not shown, and the page says how many facts were left out

#### Scenario: Wrong, remove
- **WHEN** the person removes a fact and the company is read again
- **THEN** the fact stays removed, and Undo brings it back

#### Scenario: Reused for 30 days
- **WHEN** another job at the same company is prepared within 30 days
- **THEN** its facts are the same ones, nothing is fetched and no model is called; "Read again" fetches and asks again

#### Scenario: Cut off, then half the text
- **WHEN** the model's first answer to a company's pages is cut off
- **THEN** it is asked once more with half the page text, and a company whose second answer also fails is reported as not read

### Requirement: Company reads are capped and never in the background
An account SHALL be limited to 20 company reads per UTC day, refused with
`company_reads_daily_cap` before any fetch or call; every read that starts
counts, whether or not the site answered or the model could answer, and a read that failed SHALL never be retried by itself. Calls SHALL go through the account
and instance spending limits, checked before each call. Without a model, or
over a limit, the page SHALL say so in plain words and nothing SHALL be fetched
or retried. Nothing SHALL run on a timer.

#### Scenario: Over the daily cap
- **WHEN** an account has started 20 company reads today and reads another
- **THEN** it is refused with `company_reads_daily_cap` and nothing is fetched

#### Scenario: Another person's opportunity
- **WHEN** someone asks for the brief of an opportunity that is not theirs
- **THEN** it is not found, nothing is fetched and no model is called
