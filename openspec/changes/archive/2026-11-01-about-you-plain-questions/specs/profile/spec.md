## MODIFIED Requirements

### Requirement: Past employers are read, not searched for
Researching a past employer SHALL reuse the company facts of the pipeline (one
site choice, one page reader with robots.txt and host guard, one quote-on-page
check and one 30-day cache per account and company, shared with a job's
company), through a port, for at most the five latest employers. It SHALL fetch
only when the employer's own site is known, SHALL never search the open web and
SHALL never loop. Page text SHALL be treated as data, never as instructions.
Reading employers SHALL be best effort: at most three SHALL be read at a time,
each within about 25 seconds and all within about 28 seconds, and an employer
that cannot be read (cut off, too slow, unreachable, forbidden by its robots.txt,
or an AI failure) SHALL be recorded as failed for that employer alone, with the
reason, and SHALL never stop the portrait. A failed read SHALL still count toward
the day's limit and SHALL never be retried by itself, not even when the portrait
is made again; only the person's Try again reads that employer, and only that
one, and it counts like any other read. The portrait request SHALL answer, with
the portrait or with its error, within about 40 seconds. Typing a new website for
an employer SHALL forget that its last read failed.

#### Scenario: A known site
- **WHEN** an employer's site is known and its facts are older than 30 days
- **THEN** its pages are read once, its facts are checked against their pages, and a rebuilt portrait within 30 days reads nothing again

#### Scenario: An unknown site
- **WHEN** an employer's site is not known
- **THEN** nothing is fetched, the page says so, and the person can type the site

#### Scenario: A site that cannot be read
- **WHEN** an employer's robots.txt forbids its home page
- **THEN** nothing is read for it, the page says why, and the portrait is still made

#### Scenario: One employer cannot be read
- **WHEN** the read of one employer fails and the others read
- **THEN** the portrait is made from the profile, the facts and the employers that read, that employer's row says it could not be read this time and why, and the others show what was found

#### Scenario: Try again
- **WHEN** the person chooses Try again on a failed employer
- **THEN** only that employer's website is read, it counts toward the day's limit, its row shows what came of it, and the portrait is not changed until the person updates it

#### Scenario: A failed read is not repeated by itself
- **WHEN** the portrait is made again after an employer failed
- **THEN** that employer is not read again and still says it could not be read

#### Scenario: A slow read
- **WHEN** an employer is still being read when the portrait's time for reading runs out
- **THEN** the portrait is made without it and its row says it took too long

### Requirement: Tell me about you is small, capped and never in the background
The portrait SHALL make one model call per profile version, each employer read one (a read that fails counts too, and so does Try again), each "Anything else" one, each question shown one and each answer one; no call SHALL be made in the background and settling, editing, removing, skipping, stopping, reviewing, reloading a question already on screen and saving SHALL make none. Calls SHALL go through the account and instance spending limits, checked before each call. An account SHALL be limited to 60 wizard calls per UTC day, refused with `about_daily_cap` before any call. Without a model, or over a limit, the page SHALL say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 60 wizard calls today and asks for a question
- **THEN** it is refused with `about_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and what the person wrote stays in the box

#### Scenario: A question already on screen
- **WHEN** the page is reloaded with a question showing
- **THEN** the same question is shown and no model is called

### Requirement: Tell me about you asks about the gaps, one at a time
After "Anything else", Tell me about you SHALL ask questions about what the app does not know or only assumes, one at a time, each skippable, and the person SHALL be able to stop at any point with "That's enough for now". What is asked SHALL be decided in code from the profile, the kept facts, the confirmed and wrong assumptions, the goals and the wizard's state, and never by a model. The gaps SHALL be ranked: assumptions the person left for later (and, after "Not really", those they called wrong), then the most recent role's results without a number, then empty skills, then the rest: a lead or manager title without a team size, results without a number on the other recent roles, technologies the roles name that are not skills, a skill the person told that none of their roles mention, an empty summary, headline or background, goals not answered, and each area of each recent role nothing has been kept about. What the person has told so far SHALL close the gap it answers. The company's context SHALL never be asked. Questions SHALL stop when no gap is left, after 12 (8 after "Yes, mostly", 16 after "Not really"), or when the person says so. A skipped question SHALL not come back for the same profile version. Each question SHALL show how many have been asked of about how many, and the profile's strength with "→ N% if you answer", computed from the gap's weight in the strength.

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

### Requirement: Every question says why it is asked
A question SHALL carry a reason that says what Applicamp assumed or does not know about the person, a tag (I assumed, I don't know, You told me), and, only for a closed question, choices. A question SHALL never come without a reason (the gap's own basis stands in when the model leaves it out). The model SHALL be shown only the top gap and its context (that position's entry and facts, and the related assumption), never the whole profile and never a name, email, phone, address or link; it SHALL also be shown the facts already kept about that position's employer (what it sells, to whom), and SHALL be told never to ask for anything the context already answers, and to state what it knows as an assumption to confirm. A question SHALL be one short sentence in plain everyday words, the way a friendly recruiter talks: it SHALL begin from what is known or assumed, SHALL never use jargon, never say "result" or "achieve" on its own, and never ask how the person knows. The reason SHALL be one or two plain sentences saying what is assumed or missing and why it helps. For a results gap the question SHALL ask what changed because of the person's work and offer two to four concrete example outcomes that fit that company and role as choices, with a hint that a rough number helps but may be skipped; the person can always write their own answer instead. The reason the gap itself gives when the model gives none SHALL be plain too ("Your role at Example Org says what you did, but not what changed because of it.").

#### Scenario: A reason that names the assumption
- **WHEN** a position is titled Lead Engineer and its team size is not in the profile
- **THEN** the question says, in the person's language, that the title usually means leading 3 to 6 engineers, that the profile does not say, and that recruiters ask

#### Scenario: Only the gap's context
- **WHEN** a question is worded about the second position
- **THEN** the model sees that position's words and facts and not the first position's, and no contact detail

#### Scenario: A results question in plain words
- **WHEN** the latest role at a company that sells door-access systems has no result
- **THEN** the question starts from the role and what the company makes, asks what got better because of the person's work, offers outcomes such as more properties connected or fewer support tickets to pick from, and never says "how do you know it"

#### Scenario: What the employer's site already says
- **WHEN** the employer's own site was read and says what the company sells
- **THEN** the model is shown those facts and the question does not ask what the company sells

### Requirement: An answer becomes changes, flagged and never dropped
An answer, in the person's words and the choices they picked (which SHALL be among those offered), SHALL be read with one model call (picking choices without writing anything SHALL be a valid answer) into proposed changes with the same structure and checks as "Anything else": each quotes their words, a quote that is not theirs and any claim the value adds that their words do not say SHALL be a flag, shown and never dropped, settled with It's true, Fix or Use my own words. A fact SHALL belong to the position the question was about. The changes SHALL join the pending changes (a newer summary, headline or background replaces an older one; a skill already there is not added twice), the portrait SHALL gain a told sentence for each, marked as new, and the screen SHALL say where each change goes. A new "Anything else" SHALL replace only the changes it made. Nothing SHALL be saved until Save.

#### Scenario: A number
- **WHEN** the person answers a results question with "runs went from 6 hours to 40 minutes"
- **THEN** a change is proposed on that position with their words as its quote, the portrait gains the sentence as new, the question is closed, and the profile is unchanged until Save

#### Scenario: A claim that is not theirs
- **WHEN** the model's change names a company the answer never mentions
- **THEN** the change is kept with a flag naming the company, and Save waits until it is settled

#### Scenario: A choice the question did not offer
- **WHEN** the request names a choice that is not among those shown
- **THEN** it is refused and nothing is read

#### Scenario: Only a pick
- **WHEN** the person picks "Fewer support tickets" on a results question and writes nothing
- **THEN** a change is proposed with that pick as its words and its value, no number is added, and it is not flagged

#### Scenario: A number they typed
- **WHEN** the person also types "from 40 a week to 10"
- **THEN** the number is kept as they wrote it
