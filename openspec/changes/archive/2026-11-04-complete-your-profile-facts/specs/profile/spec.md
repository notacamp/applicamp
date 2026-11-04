## ADDED Requirements

### Requirement: Complete your profile proposes the facts the app already knows
The app SHALL propose for the profile, with no model and without asking the person to write
anything again, the facts it already knows that the profile does not hold. From Tell me about you:
every assumption the person confirmed (That's right, or no answer when they said the portrait is
accurate); every sentence the portrait took from the profile that names a number or a tool its
position does not hold, when they said the portrait is accurate and it was made for that profile;
and what they wrote under "Anything else" or in an answer, as a fact on its position, a skill or
their background. From their own website: what the portrait assumed from it, confirmed the same
way, with the page it was read on. From a CV imported again: the achievements of a position that
the position does not hold. A change whose claim check failed and that the person did not settle
SHALL be left out and counted, never proposed. A proposal SHALL never repeat one already made (open,
kept or dropped) or anything the profile holds. Proposals SHALL be made when the portrait is made,
when an assumption is confirmed or the portrait is said to be accurate, when what the person wrote is
read, when a flag is settled, when an import is applied, at Save (before its pending state is
deleted), and once for the accounts that existed before. Nothing SHALL reach the profile until the
person keeps it.

#### Scenario: The portrait was said to be accurate
- **WHEN** the person answered "Yes, mostly" and left the assumption "Led a small team at Example Co (3 to 6 people)" unanswered, and never saved
- **THEN** that fact is proposed on Example Co, marked "you confirmed", with the reason it was assumed, and the profile is unchanged

#### Scenario: What they wrote
- **WHEN** the person wrote "I led 4 engineers at Example Co" under "Anything else"
- **THEN** "Led 4 engineers" is proposed on Example Co, marked "you said", with their words as its quote

#### Scenario: Wrong and later
- **WHEN** the person chose That's wrong or Ask me later on an assumption
- **THEN** it is not proposed

#### Scenario: A claim nobody settled
- **WHEN** a change says "Cut refunds by 80%" and neither their words nor their profile say 80%
- **THEN** it is not proposed, and it is counted

#### Scenario: From their own website
- **WHEN** the portrait assumed from the person's site that they built an open-source billing toolkit, and they confirmed it
- **THEN** it is proposed marked "from your website", with the page it was read on

#### Scenario: Save
- **WHEN** the person saves Tell me about you with assumptions left unanswered after "Yes, mostly"
- **THEN** Save writes what it always wrote, and those assumptions wait on Complete your profile

#### Scenario: Never twice
- **WHEN** the person dropped "Elixir" and the app looks again
- **THEN** it is not proposed again

#### Scenario: Before this change
- **WHEN** the server starts for the first time with this change, and an account has a pending Tell me about you
- **THEN** its facts are proposed once, and never again by a later start

### Requirement: Keep, Drop and Undo on Complete your profile
The page "Complete your profile" SHALL list the proposals by job (newest first), then skills, then
background. Each SHALL show its text, where it comes from (you said, you confirmed, from your
website, from your CV) and the words it comes from. The page SHALL show a progress line "N of M
decided" and Keep all. Each proposal SHALL have Keep and Drop, both one tap. Keep SHALL write the
fact on its job with its quote (a confirmed assumption marked as such, with what it was based on),
or add the skill, or add the words to the background, as one new profile version; Keep all SHALL
be one version too. A kept or dropped proposal SHALL stay in place, kept in green and dropped faded,
with Undo for a day; Undo SHALL take back exactly what Keep wrote and nothing else. When all are
decided, one line SHALL say how many facts were added and that this can be undone for a day. When
an import was applied in the last day, the page SHALL say what it found. Profile & goals SHALL lead
to the page while facts wait. Buttons SHALL be at least 44 px tall on a phone and 36 px on a
desktop, and no label SHALL carry a price or a keyboard hint.

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
- **THEN** all seven are on the profile as one new version, and one line says "7 facts added to your profile. You can undo this for a day."

#### Scenario: A job that changed
- **WHEN** the job a fact was proposed for was removed or renamed since
- **THEN** that fact is not shown and cannot be kept

#### Scenario: Another account
- **WHEN** another account asks for or decides one of these proposals
- **THEN** it is not found
