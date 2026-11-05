## ADDED Requirements

### Requirement: Complete your profile proposes the facts the app already knows
The app SHALL propose for the profile, with no model and without asking the person to write
anything again, the facts it already knows that the profile does not hold. From Tell me about you:
every assumption the person confirmed (That's right, or no answer when they said that very
portrait is accurate; an answer to "Is this accurate?" SHALL count only for the portrait it was
given on); every sentence the portrait took from a position, or from the person's own words, that
names a number or a tool the position does not hold, when they said the portrait is accurate and
it was made for that profile (never one taken from the goals, the years or the skills list); and
what they wrote under "Anything else" or in an answer, as a fact on its position, a skill or their
background. From their own website: what the portrait assumed from it, confirmed the same way, with
the page it was read on. From a CV imported again: the achievements of a position, matched as Merge
matches positions, that the position does not hold. An item about no position SHALL go on the
newest one. A change whose quote is not the person's words, or whose claim check failed and that
they did not settle, SHALL be left out and counted, never proposed. A proposal SHALL never repeat
one already made (open, kept or dropped) or anything the profile holds, whatever its closing
punctuation or a renamed title. A waiting proposal SHALL be closed when the person takes it back in
Tell me about you (an assumption switched to wrong or later, an answer changed, a change edited or
removed, a new portrait) or when the profile comes to hold it. Proposals SHALL be made whenever
Tell me about you's state changes, when an import is applied, at Save (before its pending state is
deleted), and once for the accounts that existed before; an account that fails SHALL not stop the
others. Nothing SHALL reach the profile until the person keeps it.

#### Scenario: The portrait was said to be accurate
- **WHEN** the person answered "Yes, mostly" and left the assumption "Led a small team at Example Co (3 to 6 people)" unanswered, and never saved
- **THEN** that fact is proposed on Example Co, marked "you confirmed", with the reason it was assumed, and the profile is unchanged

#### Scenario: A new portrait
- **WHEN** the portrait is made again for a new profile version
- **THEN** "Is this accurate?" is asked again, and nothing of the new portrait is proposed until the person answers it or confirms an assumption

#### Scenario: What they wrote
- **WHEN** the person wrote "I led 4 engineers at Example Co" under "Anything else"
- **THEN** "Led 4 engineers" is proposed on Example Co, marked "you said", with their words as its quote

#### Scenario: Taken back
- **WHEN** the person edits that change to "Led 4 people", or switches an assumption to That's wrong
- **THEN** the old fact no longer waits, and the edited one does

#### Scenario: Wrong and later
- **WHEN** the person chose That's wrong or Ask me later on an assumption
- **THEN** it is not proposed

#### Scenario: A claim nobody settled, a quote not theirs
- **WHEN** a change says "Cut refunds by 80%" and neither their words nor their profile say 80%, or its quote is not in their words even after It's true
- **THEN** it is not proposed, and it is counted

#### Scenario: Not from the goals
- **WHEN** the portrait says a past job used Rust because the goals say the next job must have Rust
- **THEN** it is not proposed

#### Scenario: From their own website
- **WHEN** the portrait assumed from the person's site that they built an open-source billing toolkit, and they confirmed it
- **THEN** it is proposed marked "from your website", with the page it was read on

#### Scenario: Save
- **WHEN** the person saves Tell me about you with assumptions left unanswered after "Yes, mostly", and with a fact they dropped on Complete your profile
- **THEN** Save writes what it always wrote but not the dropped fact, and the unanswered assumptions wait on Complete your profile

#### Scenario: Never twice
- **WHEN** the person dropped "Elixir" and the app looks again
- **THEN** it is not proposed again

#### Scenario: Before this change
- **WHEN** the server starts for the first time with this change, and an account has a pending Tell me about you
- **THEN** its facts are proposed once, and never again by a later start

### Requirement: Keep, Drop and Undo on Complete your profile
The page "Complete your profile" SHALL list the proposals by job (newest first), then skills, then
background. Each SHALL show its text, where it comes from (you said, you confirmed, from your
website, from your CV) and the words it comes from; "You said this was right" SHALL appear only
under an assumption the person marked right. The page SHALL show a progress line "N of M decided"
and Keep all. Each proposal SHALL have Keep and Drop, both one tap. Keep SHALL write the fact on its
job with its quote (a confirmed assumption marked as such, with what it was based on), or add the
skill (forgetting an earlier "not mine"), or add the words to the background, as one new profile
version; Keep all SHALL be one version too. A fact the profile already holds SHALL be closed, not
kept, and nothing written. A kept or dropped proposal SHALL stay in place, kept in green and
dropped faded, with Undo for a day; Undo SHALL take back exactly what Keep wrote and nothing the
profile had before (the background only while it still ends with the words Keep added; a skill
with its "not mine" restored). When all are decided, one line SHALL say how many facts were added
and that this can be undone for a day, with one Undo for the whole Keep all. For a day after an
import was applied the page SHALL say what it found. Profile & goals SHALL lead to the page while
facts wait, and so SHALL Tell me about you's saved screen. Buttons SHALL be at least 44 px tall on a
phone and 36 px on a desktop, and no label SHALL carry a price or a keyboard hint.

#### Scenario: Keep one
- **WHEN** the person keeps "Led 4 engineers" on Example Co
- **THEN** it is a fact on that job with their words as its quote, the profile gains one version, and the card turns green with "Kept" and Undo

#### Scenario: Drop one
- **WHEN** the person drops a fact
- **THEN** nothing is written, and the card fades with "Dropped" and Undo

#### Scenario: Undo
- **WHEN** the person undoes a fact kept an hour ago
- **THEN** it is off the job again and waits for Keep or Drop; and **WHEN** the decision is more than a day old, Undo is refused

#### Scenario: Keep all
- **WHEN** the person chooses Keep all with seven facts waiting
- **THEN** all seven are on the profile as one new version, one line says "7 facts added to your profile. You can undo this for a day.", and its Undo takes all seven back as one version

#### Scenario: Already on the profile
- **WHEN** the person keeps a fact their profile gained by hand since
- **THEN** it is closed, nothing is written, and it is not counted as added

#### Scenario: The background changed since
- **WHEN** the person kept words for their background, and Save then wrote a new background
- **THEN** Undo leaves the background as Save wrote it and the fact waits again

#### Scenario: A job that changed
- **WHEN** the job a fact was proposed for was removed since
- **THEN** that fact is not shown and cannot be kept; a job whose title was renamed keeps its facts

#### Scenario: Another account
- **WHEN** another account asks for or decides one of these proposals
- **THEN** it is not found
