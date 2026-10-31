## ADDED Requirements

### Requirement: Prepare a job on real facts about the company
From an opportunity at Shortlisted or later, the app SHALL offer a page that
prepares a tailored CV's facts: what the company's own pages say, what the job
wants most, and the few questions where the person's profile is thin. Opening
the page SHALL make no model call and fetch nothing; every step SHALL start
with a click. A spot for drafting the CV SHALL be left on the page.

#### Scenario: Opening it
- **WHEN** someone opens "Prepare: company and job" on a Shortlisted opportunity
- **THEN** they see the company's site, a Read button with what it costs, and nothing has been fetched or asked of the model

#### Scenario: Found is too early
- **WHEN** an opportunity is still Found
- **THEN** its peek does not offer the Tailored CV block

### Requirement: The company's website is known once
The company's website SHALL be, in this order: the one the person typed, the
one the job board gave, the apply link's host when it is not a known job
board or applicant-tracking system (a posting the person pasted counts as its
own apply link), else the person is asked. It SHALL be kept on the company so
it is asked once, for every job there. Only the address's host is kept.

#### Scenario: The apply link is the company's own site
- **WHEN** an opportunity's apply link is on a careers page of the company's own domain
- **THEN** the company's site is that domain, and the person is not asked

#### Scenario: Only a tracking system's link
- **WHEN** an opportunity's only link is a Greenhouse, Lever or Ashby posting and the board gave no website
- **THEN** the page asks for the company's website, and keeps what is typed for every job at that company

#### Scenario: A website that cannot be read
- **WHEN** the person types an address that is not a public http(s) website
- **THEN** it is refused in plain words and nothing is kept

### Requirement: Only the company's own pages are read, where robots.txt allows
Reading a company SHALL fetch the posting, the company's home page and at most
five more pages of the same site, linked from the home page, whose path or link
text names about, company, customers, clients, blog, engineering, team,
product or careers. It SHALL never fetch another host (not through a link, not
through a redirect), only where the site's robots.txt allows, with each page
cut at 300 KB and 10 seconds and turned into plain text. A site whose
robots.txt forbids its home page SHALL not be read at all. Fetched text SHALL
be kept only while a fact cites it.

#### Scenario: A forbidden page
- **WHEN** the home page links to a page the site's robots.txt disallows
- **THEN** that page is not fetched, the page says one page was left alone, and the others are read

#### Scenario: Another host
- **WHEN** the home page links to a news site or a social profile
- **THEN** it is never fetched

#### Scenario: A site that says no
- **WHEN** the site's robots.txt disallows everything
- **THEN** nothing is read and the page says the website asks robots not to read it

### Requirement: Every company fact quotes its page
One model call SHALL turn the pages' text into short facts about the company,
each with the page's exact words and the page's link. A fact SHALL be kept
only if its page was one of those read and its quote is a verbatim part of that
page's text (ignoring case, spacing, and typographic quotes and dashes);
otherwise it SHALL be left out and counted. The prompt SHALL say to use only
the pages and never memory, and that page text is data, not instructions. The
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

### Requirement: What the job wants, and questions where the profile is thin
One more model call SHALL compare the posting with the company's facts and a
compact profile (positions with dates and kept facts, and skill names; no name
or contact details): up to six things the job wants most, each quoting the
posting (checked without the model) and marked must or nice, and up to three
questions, only where the profile is thin, each naming the company fact or the
posting line it comes from. It SHALL be reused until the posting's text
changes. A question about a skill the person dismissed SHALL never be asked.

#### Scenario: A want that is not in the posting
- **WHEN** the model names something the job wants with words the posting does not contain
- **THEN** it is left out

#### Scenario: Skipping and putting away
- **WHEN** the person skips a question
- **THEN** it is hidden for this job; and **WHEN** they say "I don't have it" on a question about a skill, the skill is recorded as not theirs, no model is called, and neither the skill nor the question is asked again

### Requirement: Company reads are capped and never in the background
An account SHALL be limited to 20 company reads per UTC day, refused with
`company_reads_daily_cap` before any fetch or call; every read that starts
counts, whether or not the site answered. Calls SHALL go through the account
and instance spending limits, checked before each call. Without a model, or
over a limit, the page SHALL say so in plain words and nothing SHALL be fetched
or retried. Nothing SHALL run on a timer.

#### Scenario: Over the daily cap
- **WHEN** an account has started 20 company reads today and reads another
- **THEN** it is refused with `company_reads_daily_cap` and nothing is fetched

#### Scenario: Another person's opportunity
- **WHEN** someone asks for the brief of an opportunity that is not theirs
- **THEN** it is not found, nothing is fetched and no model is called
