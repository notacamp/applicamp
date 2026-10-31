## ADDED Requirements

### Requirement: Tell me about you asks about the gaps, one at a time
After "Anything else", Tell me about you SHALL ask questions about what the app does not know or only assumes, one at a time, each skippable, and the person SHALL be able to stop at any point with "That's enough for now". What is asked SHALL be decided in code from the profile, the kept facts, the confirmed and wrong assumptions, the goals and the wizard's state, and never by a model. The gaps SHALL be ranked: assumptions the person left for later (and, after "Not really", those they called wrong), then the most recent role's results without a number, then empty skills, then the rest: a lead or manager title without a team size, results without a number on the other recent roles, technologies the roles name that are not skills, a skill the person told that none of their roles mention, an empty summary, headline or background, goals not answered, and each area of each recent role nothing has been kept about. What the person has told so far SHALL close the gap it answers. The company's context SHALL never be asked. Questions SHALL stop when no gap is left, after 12 (8 after "Yes, mostly", 16 after "Not really"), or when the person says so. A skipped question SHALL not come back for the same profile version. Each question SHALL show how many have been asked of about how many, and the profile's strength with "→ N% if you answer", computed from the gap's weight in the strength.

#### Scenario: An assumption left for later is asked first
- **WHEN** the person chose Ask me later on "I assume you shipped pipelines" and continues
- **THEN** the first question is about it, saying what was assumed and why

#### Scenario: A role without a number
- **WHEN** the most recent role states no result with a number
- **THEN** a question about one result with a number comes next, tagged I don't know, with the strength it would add

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
A question SHALL carry a reason that says what Applicamp assumed or does not know about the person, a tag (I assumed, I don't know, You told me), and, only for a closed question, choices. A question SHALL never come without a reason (the gap's own basis stands in when the model leaves it out). The model SHALL be shown only the top gap and its context (that position's entry and facts, and the related assumption), never the whole profile and never a name, email, phone, address or link; it SHALL be told never to ask for anything it knows or that can be looked up, and to state what it knows as an assumption to confirm.

#### Scenario: A reason that names the assumption
- **WHEN** a position is titled Lead Engineer and its team size is not in the profile
- **THEN** the question says, in the person's language, that the title usually means leading 3 to 6 engineers, that the profile does not say, and that recruiters ask

#### Scenario: Only the gap's context
- **WHEN** a question is worded about the second position
- **THEN** the model sees that position's words and facts and not the first position's, and no contact detail

### Requirement: An answer becomes changes, flagged and never dropped
An answer, in the person's words and the choices they picked (which SHALL be among those offered), SHALL be read with one model call into proposed changes with the same structure and checks as "Anything else": each quotes their words, a quote that is not theirs and any claim the value adds that their words do not say SHALL be a flag, shown and never dropped, settled with It's true, Fix or Use my own words. A fact SHALL belong to the position the question was about. The changes SHALL join the pending changes (a newer summary, headline or background replaces an older one; a skill already there is not added twice), the portrait SHALL gain a told sentence for each, marked as new, and the screen SHALL say where each change goes. A new "Anything else" SHALL replace only the changes it made. Nothing SHALL be saved until Save.

#### Scenario: A number
- **WHEN** the person answers a results question with "runs went from 6 hours to 40 minutes"
- **THEN** a change is proposed on that position with their words as its quote, the portrait gains the sentence as new, the question is closed, and the profile is unchanged until Save

#### Scenario: A claim that is not theirs
- **WHEN** the model's change names a company the answer never mentions
- **THEN** the change is kept with a flag naming the company, and Save waits until it is settled

#### Scenario: A choice the question did not offer
- **WHEN** the request names a choice that is not among those shown
- **THEN** it is refused and nothing is read

### Requirement: Improve my profile, Skills and Grill me are retired into Tell me about you
The routes `/profile/improve`, `/profile/skills` and `/profile/grill` (and `/profile/grill/{position}`) SHALL lead to Tell me about you. Every entry point to them SHALL lead there instead: the Grill me button, the per-role "Grill me about this role" links, Today's nudge and the strength checklist's Add and Improve buttons for results, skills and questions (a part with a place on the profile, such as the headline or a link, is still added in place). The facts the person kept SHALL stay on the profile under "What you told Applicamp about each role", and the skills SHALL be reordered from the profile's own skills list. Skills the roles name that the skills lack SHALL be a question, not a page.

#### Scenario: An old address
- **WHEN** someone opens `/profile/grill` or `/profile/improve`
- **THEN** they land on Tell me about you

#### Scenario: A role's kept facts
- **WHEN** the person opens Profile & goals
- **THEN** each role lists the facts they told Applicamp about it, with their own words and Remove, under "What you told Applicamp about each role", and no Grill me button

#### Scenario: Skills in order
- **WHEN** the person moves a skill up in the profile's skills list
- **THEN** it is listed where they put it, the first three are marked as the ones the job board searches use, and nothing else changes

### Requirement: The profile's strength leads to Tell me about you
The app SHALL show how strong the profile is and what is missing, and lead the person to Tell me about you as the way to make it stronger, which asks about the gaps one at a time. A fact the person kept about a position's results SHALL count toward the strength like a highlight. Nothing SHALL be saved to the profile until they save, and the flow SHALL never block the rest of the app.

#### Scenario: Skills already in the roles
- **WHEN** "Rust" appears in four position descriptions but not in the skills list
- **THEN** Tell me about you asks about it ("Your roles mention Rust, but it isn't a skill") and adds it only when the person says so and saves

#### Scenario: Skills in order
- **WHEN** the person moves a skill up in the profile's skills list
- **THEN** it is listed where they put it, the first three are marked as the ones the job board searches use, and nothing is added to or removed from the list by the move

#### Scenario: A result raises the strength
- **WHEN** the person answers a results question with a number and saves
- **THEN** the profile strength rises by what the question said

#### Scenario: Not now
- **WHEN** the person chooses "Not now" on Today's profile strength card
- **THEN** it is hidden for a week, and every other part of the app works as before

#### Scenario: The way in
- **WHEN** the person opens Profile & goals, or follows Today's profile card, or chooses a part of the checklist that needs more from them
- **THEN** they arrive at Tell me about you

## MODIFIED Requirements

### Requirement: Tell me about you is small, capped and never in the background
The portrait SHALL make one model call per profile version, each employer read one, each "Anything else" one, each question shown one and each answer one; no call SHALL be made in the background and settling, editing, removing, skipping, stopping, reviewing, reloading a question already on screen and saving SHALL make none. Calls SHALL go through the account and instance spending limits, checked before each call. An account SHALL be limited to 60 wizard calls per UTC day, refused with `about_daily_cap` before any call. Without a model, or over a limit, the page SHALL say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 60 wizard calls today and asks for a question
- **THEN** it is refused with `about_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and what the person wrote stays in the box

#### Scenario: A question already on screen
- **WHEN** the page is reloaded with a question showing
- **THEN** the same question is shown and no model is called

## REMOVED Requirements

### Requirement: The profile keeps getting stronger
**Reason**: Its questions, drafts and Skills page are retired; the strength and the way to make it stronger are described by "The profile's strength leads to Tell me about you".
**Migration**: Tell me about you asks about the gaps; the strength card links there.

### Requirement: A draft is checked against the person's own words
**Reason**: Improve my profile and its "Draft from your answer" are retired; answers are read by Tell me about you, with the checks of "An answer becomes changes, flagged and never dropped".
**Migration**: The draft endpoints are deprecated and no longer called; a follow-up removes them.

### Requirement: Grill me, a deep interview about each role
**Reason**: Grill me is retired as a flow; its questions are replaced by Tell me about you's questions about the gaps, each saying why, and its kept facts stay on the profile.
**Migration**: `/profile/grill*` leads to Tell me about you. The interview endpoints are deprecated and no longer called; a follow-up removes them.

### Requirement: Grill me is small, capped and never in the background
**Reason**: The interview is retired; the wizard's own cap covers the questions.
**Migration**: See "Tell me about you is small, capped and never in the background".

### Requirement: Interviews belong to the role they were started for
**Reason**: There are no interviews any more; answers belong to the position a question names.
**Migration**: See "An answer becomes changes, flagged and never dropped".
