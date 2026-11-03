## ADDED Requirements

### Requirement: A shortlisted job is prepared by itself, waiting only on the person
When an opportunity moves from Found to Shortlisted or later, and the person's Autonomy goal is
"Prepare" or not answered, the app SHALL by itself read the company and compare the job, then
stop while a question of the brief is open, then draft the CV. Approving the CV SHALL draft the
cover letter and the job's answers by itself. Opening Prepare on a Shortlisted or later
opportunity that was never prepared SHALL start the same chain. Answering questions, keeping
facts, approving the CV and the letter, and every stage move SHALL still wait on the person.
With the goal "Find only", or without a model, nothing SHALL run by itself and every step SHALL
start with a click, as before. A step SHALL never run twice for the same opportunity at once.

#### Scenario: Shortlisting a job
- **WHEN** someone with the goal "Prepare" moves a job to Shortlisted, its company's site is known and the comparison asks no question
- **THEN** the company is read, the job compared and a CV drafted without another click, and the CV waits for their approval

#### Scenario: A question is open
- **WHEN** the comparison asks a question
- **THEN** no CV is drafted until the person answers or skips every question, and the job shows that it waits on them

#### Scenario: Approving the CV
- **WHEN** the person approves the CV of a job being prepared by itself
- **THEN** the cover letter and the job's answers are drafted and wait for their approval

#### Scenario: Find only
- **WHEN** someone whose goal is "Find only" shortlists a job
- **THEN** nothing is fetched and no model is called until they click

#### Scenario: Jobs shortlisted earlier
- **WHEN** someone with the goal "Prepare" opens Prepare on a Shortlisted job that was never prepared
- **THEN** the chain starts

### Requirement: The person sees and stops what runs by itself
Prepare SHALL show each step done by itself, with what it cost and when, and what the chain
waits on. Stop SHALL cancel every step still to run for that job; so SHALL moving it back to
Found or Not for me, or choosing "Find only". "Prepare by itself" SHALL start it again. Read
again, Draft again and Try again SHALL stay. A step that failed SHALL never be retried by itself.
A step a cap or a spending limit would pass SHALL not start: the page SHALL say why and offer
the step as a click. Today SHALL list the jobs waiting on the person's answers or approval.

#### Scenario: Stop
- **WHEN** someone stops a job's preparation while the CV draft is still to run
- **THEN** no draft is made, and the page offers "Prepare by itself"

#### Scenario: A cap
- **WHEN** the daily CV draft cap is reached while a job is being prepared
- **THEN** no draft is made, the job says the cap stopped it, and "Draft the CV" is offered for tomorrow

#### Scenario: A failure
- **WHEN** reading the company fails
- **THEN** the job says why, offers Try again, and nothing is retried by itself

#### Scenario: Another person's job
- **WHEN** someone stops or starts the preparation of an opportunity that is not theirs
- **THEN** it is not found

## MODIFIED Requirements

### Requirement: Prepare a job on real facts about the company
From an opportunity at Shortlisted or later, the app SHALL offer a page that
prepares a tailored CV's facts: what the company's own pages say, what the job
wants most, and the few questions where the person's profile is thin. Opening
the page SHALL make no model call and fetch nothing unless it starts the chain
of a job that was never prepared (see "A shortlisted job is prepared by itself").
The page SHALL offer "Draft the CV" with what it costs, or "Review the CV" when a
draft exists, and say why drafting is off when it is. "Draft the CV" on a job
whose company was never read SHALL read it first when it can, and say so when it
cannot.

#### Scenario: Opening it
- **WHEN** someone whose goal is "Find only" opens "Prepare: company and job" on a Shortlisted opportunity
- **THEN** they see the company's site, a Read button with what it costs, a Draft the CV button with what it costs, and nothing has been fetched or asked of the model

#### Scenario: Found is too early
- **WHEN** an opportunity is still Found
- **THEN** its peek does not offer the Tailored CV block, and drafting a CV for it is refused with `stage_too_early`

#### Scenario: Drafting before reading
- **WHEN** someone clicks "Draft the CV" on a job whose company's site is known but was never read
- **THEN** the company is read and the job compared first, and the draft uses what the job wants

### Requirement: The company's website is known once
The company's website SHALL be, in this order: the one the person typed, the
one the job board gave, the apply link's host when it is not a known job
board or applicant-tracking system (a posting the person pasted counts as its
own apply link), a link or email address in the posting whose domain is the
company's, a domain made from the company's name whose robots.txt allows its
home page and whose home page names the company (at most three tried, with no
model call), else the person is asked. Job boards, tracking systems, webmail
and link shorteners SHALL never count. The page SHALL say where the site came
from, and the person's address SHALL always win and be read again. It SHALL be
kept on the company so it is found or asked once, for every job there. Only the
address's host is kept.

#### Scenario: The apply link is the company's own site
- **WHEN** an opportunity's apply link is on a careers page of the company's own domain
- **THEN** the company's site is that domain, and the person is not asked

#### Scenario: An email address in the posting
- **WHEN** an opportunity's only link is a Lever posting and its text says to write to `jobs@example-co.com`
- **THEN** the company's site is `https://example-co.com/`, the page says it was found in the posting's email address, and the person is not asked

#### Scenario: A domain from the name
- **WHEN** the posting names no domain, and `examplesoft.com`'s home page names "ExampleSoft Software" and its robots.txt allows it
- **THEN** that is the company's site, and the page says it was found from the company's name

#### Scenario: Only a tracking system's link
- **WHEN** an opportunity's only link is a Greenhouse, Lever or Ashby posting, the board gave no website, the posting names no domain of the company's, and no domain made from the name has a home page that names the company
- **THEN** none of those guesses is used, the page asks for the company's website, and keeps what is typed for every job at that company

#### Scenario: Webmail is not a website
- **WHEN** the posting's only address is at a webmail domain
- **THEN** that domain is never the company's site

#### Scenario: The person corrects it
- **WHEN** the person types another website for a company whose site was found
- **THEN** their address is kept for every job there and the company is read again from it

#### Scenario: A website that cannot be read
- **WHEN** the person types an address that is not a public http(s) website
- **THEN** it is refused in plain words and nothing is kept

### Requirement: Company reads are capped and never in the background
An account SHALL be limited to 20 company reads per UTC day, refused with
`company_reads_daily_cap` before any fetch or call; every read that starts
counts, whether or not the site answered or the model could answer, and a read
that failed SHALL never be retried by itself. A read SHALL start only from a
click or from the preparation of a job the person shortlisted. Calls SHALL go
through the account and instance spending limits, checked before each call.
Without a model, or over a limit, the page SHALL say so in plain words and
nothing SHALL be fetched or retried. Nothing SHALL run on a timer.

#### Scenario: Over the daily cap
- **WHEN** an account has started 20 company reads today and reads another
- **THEN** it is refused with `company_reads_daily_cap` and nothing is fetched

#### Scenario: Another person's opportunity
- **WHEN** someone asks for the brief of an opportunity that is not theirs
- **THEN** it is not found, nothing is fetched and no model is called

### Requirement: Draft a CV from the profile, every line citing its sources
From a Shortlisted or later opportunity, one click, or the preparation of a job
the person shortlisted, SHALL make a draft CV with
one model call: a headline, a summary, positions with bullets and skills, each
text citing the ids of the profile sources it comes from (a position's summary,
a highlight, a kept fact, the summary, the headline, the skills list,
education, languages) and, in the summary only, the company facts the person
has not removed. The model SHALL be shown the posting, what the job wants, the
company facts and the profile as data, and SHALL never be shown the person's
name, email, address, links or any contact detail: the server SHALL print those
itself. The draft SHALL be written in the language asked for, else the
posting's, else English.

#### Scenario: A draft
- **WHEN** someone drafts a CV for a Shortlisted opportunity
- **THEN** the draft has a number, a language and lines that each cite sources, the contact is the profile's, and the prompt sent to the model holds no name or contact detail

#### Scenario: Not Shortlisted yet
- **WHEN** someone drafts a CV for an opportunity still at Found
- **THEN** it is refused with `stage_too_early` and the model is not called

#### Scenario: Another person's opportunity
- **WHEN** someone drafts, reads or changes the CV of an opportunity that is not theirs
- **THEN** it is not found

### Requirement: Drafts are capped, expire and never run in the background
An opportunity SHALL have at most 10 drafts and an account at most 20 drafts per
UTC day, refused with `cv_drafts_per_opportunity_cap` and `cv_drafts_daily_cap`
before any call; drafts made by a job's preparation count the same. A draft
SHALL start only from a click or from the preparation of a job the person
shortlisted. The call SHALL go through the account and instance spending
limits, and nothing SHALL be retried past a limit or run on a timer. A draft
nobody approved SHALL be deleted 7 days after its last change; approved versions
SHALL be kept.

#### Scenario: Over the daily cap
- **WHEN** an account has made 20 drafts today and drafts another
- **THEN** it is refused with `cv_drafts_daily_cap` and the model is not called

#### Scenario: Left alone
- **WHEN** a draft is not approved and not changed for 7 days
- **THEN** it is gone, while an approved version stays

### Requirement: A cover letter needs an approved CV and one model call
For an opportunity whose CV is approved, a click, or approving the CV of a job being prepared by
itself, SHALL write a cover letter with exactly one model call that is
shown the posting, what the job wants, the company facts the person kept, the profile with every kept fact, the
words of the approved CV, the language, the length (short, about 180 words, by default; standard, about 300) and
the job's own questions. It SHALL never be shown a name or contact details: the server prints the greeting, the
name and the contact. The prompt SHALL ban "I am writing to apply", flattery, clichés and superlatives the sources
do not support. Without an approved CV the action SHALL be off and say why.

#### Scenario: No approved CV
- **WHEN** the CV of a job is not approved
- **THEN** "Write the cover letter" is off with "Approve your CV for this job first", and no model is called

#### Scenario: Nothing private is sent
- **WHEN** a letter is drafted
- **THEN** the model's message holds no name, email, address or link of the person
