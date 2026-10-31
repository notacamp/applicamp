## ADDED Requirements

### Requirement: Grill me, a deep interview about each role
The app SHALL offer an interview about one role at a time that asks one
question at a time, in the person's language, through seven areas in order
(context, their part, stack and scale, results, hard problems, people,
stories). It SHALL push back on vague answers, ask for numbers and how the
person knows them, ask what they did themselves as against the team, and ask
about contradictions between the new answer, earlier facts and the profile.
Every question SHALL be skippable, the person SHALL be able to leave a role
for now and continue later, and nothing SHALL be saved to the profile until
they keep it.

#### Scenario: A vague answer
- **WHEN** the person answers a question with "I did the upgrade, it went well"
- **THEN** the next question is marked as pushing back and asks who decided, how many people worked on it and how long it took, and no fact is proposed

#### Scenario: A real answer
- **WHEN** the person answers "I proposed it and led it. 2 others helped part time. It took 7 months."
- **THEN** they are shown short facts, each with their exact words, with Keep, Fix and Drop, and the areas that answer covered go up

#### Scenario: A contradiction
- **WHEN** an answer says 12 months where an earlier fact says 7
- **THEN** the next question asks which is right, and pushes back

#### Scenario: Skip and done
- **WHEN** the person skips a question
- **THEN** that area is left alone until they start the role again, and the next question comes; and **WHEN** they choose Done for now, nothing more is asked and no model is called

#### Scenario: Coming back
- **WHEN** the person opens a role with a question waiting
- **THEN** the same question is shown and no model is called; and a role with all seven areas covered asks nothing more

### Requirement: Facts carry the person's own words
A fact SHALL be proposed only if its quote is a verbatim part of the
person's answer (ignoring case, spacing, and typographic quotes and dashes)
and its text says no number, name or tool that neither the quote nor the
profile says. A fact that fails either check SHALL be left out, counted and
never shown. Coverage SHALL only ever go up. A kept fact SHALL be saved on
that role in the profile, with its quote, as evidence for every later check.

#### Scenario: A made-up quote
- **WHEN** the model proposes a fact whose quote the person never wrote
- **THEN** it is left out and the person is told one line was left out

#### Scenario: A claim nobody made
- **WHEN** the model proposes "Took 9 months" with the quote "It took 7 months"
- **THEN** it is left out

#### Scenario: Fix
- **WHEN** the person fixes a fact to "Two others helped part time"
- **THEN** it is kept with that text and the original quote; and **WHEN** they write a number their words and profile do not say, keeping is refused naming the words and nothing is saved

#### Scenario: Keep all
- **WHEN** the person chooses Keep all with three facts waiting
- **THEN** all three are saved on the role as one profile version

#### Scenario: Remove and undo
- **WHEN** the person removes a kept fact from the profile and chooses Undo
- **THEN** it is back on the role, with its quote

#### Scenario: Waiting too long
- **WHEN** a fact has waited 24 hours without Keep, Fix or Drop
- **THEN** it is deleted and can no longer be kept

### Requirement: Grill me is small, capped and never in the background
Each start, answer or skip SHALL make at most one model call, and no call
SHALL be made in the background. Calls SHALL go through the account and
instance spending limits, which are checked before each call. An account
SHALL be limited to 150 Grill me calls per UTC day, refused with
`grill_daily_cap` before any call. Without a model, or over a limit, the page
SHALL say so in plain words and nothing SHALL be retried.

#### Scenario: Over the daily cap
- **WHEN** an account has made 150 Grill me calls today and answers
- **THEN** the answer is refused with `grill_daily_cap` and the model is not called

#### Scenario: Over a spending limit
- **WHEN** a call would cross the account's day or month limit
- **THEN** it is refused before it is sent, the page says the limit was reached, and the answer typed stays in the box

#### Scenario: No model
- **WHEN** no AI model is configured
- **THEN** the roles list still opens, and starting an interview says AI is not available

### Requirement: Interviews belong to the role they were started for
An interview SHALL be tied to the position it was started for, so that
editing, removing or reordering positions never shows one role's interview,
or keeps one role's facts, on another.

#### Scenario: A position moved
- **WHEN** the person's positions are reordered after an interview
- **THEN** the position now at that place shows as not started
