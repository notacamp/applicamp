# Complete your profile: the facts the app already knows, to keep or drop

Issue: https://github.com/notacamp/applicamp/issues/72 (part 2 of 2; part 1 is
`import-reads-every-section`)
Module: profile (new slice `features/facts`, a migration, hooks in `features/about` and
`features/imports`), server (`main.rs` queues the one-off backfill at start), the client's profile
module (a page, a nudge on Profile & goals), one route and its registration.

## Why
Tell me about you learns a lot. It writes a portrait of what the profile says, what the person
told and what it assumed, the person answers it, and the person's own website is read as
assumptions to confirm. But none of this reaches a position unless the person reaches Save. The
owner said the portrait was accurate, and still no fact was written to any position. The CV drafter
then sees only position summaries and the skills list. The owner also asked that facts have **Keep
and Drop**, not only Drop, and that it stays simple. Prototype approved:
https://claude.ai/artifact/EvmATK6pRo4qYFtLQKotfr (screen "Complete your profile").

## What changes
- **The app proposes, by itself, the facts it already knows** that the profile does not hold. It
  uses plain code and no model, and it never asks the person to type anything again:
  - from Tell me about you:
    - each assumption the person confirmed: *That's right*, or no answer when they said this same
      portrait is accurate. An answer to "Is this accurate?" counts only for the portrait it was
      given on;
    - each portrait sentence taken from a job, or from the person's own words, that names a
      number or a tool the job does not hold yet. Never one taken from the goals, the years or the
      skills list;
    - what they wrote under "Anything else" or in an answer, as a fact on a job, a skill or their
      background. A change whose quote is not their words, or whose claim check failed and was not
      settled, is left out and counted, never shown;
  - from their own website: what the portrait assumed from it, confirmed the same way, with the
    page it was read on;
  - from a CV read again: an achievement its job does not hold yet, matched to the job as Merge
    matches it.
  An item about no particular job goes on the newest one.
- **Never twice, and never what was taken back.** A fact is not proposed again once proposed,
  dropped or held by the profile, whatever its closing full stop or a renamed job title. A waiting
  fact is closed when the person takes it back in Tell me about you: an assumption switched to
  wrong or later, an answer changed, a change edited or removed, a new portrait. **Save never
  writes a fact the person dropped here** (their correction wins).
- **When**: the job `profile.propose_facts` (ids only, free) runs whenever Tell me about you's
  state changes (the portrait made, a choice, "Is this accurate?", "Anything else", an answer, a
  change settled or removed) and when an import is applied. At Save, Tell me about you proposes
  what it does not write itself, in the same transaction, before its pending state is deleted.
  Once per instance, `profile.backfill_facts` does the same for the accounts that existed before.
  An account that fails is logged and skipped, and a marker stops the backfill from running again.
- **A page, "Complete your profile"** (`/profile/complete`):
  - "We read your CV again. Found 6 jobs, 18 achievements…" for a day after an import was
    applied;
  - a progress line "N of M decided" with a bar, and **Keep all**;
  - groups by job ("Example Co · 2021 – now"), then skills, then background;
  - one card per fact, with the text, a small tag for where it comes from (*you said*, *you
    confirmed*, *from your website*, *from your CV*) and the words it comes from. "You said this
    was right" appears only under an assumption the person marked right;
  - **Keep** and **Drop** on each card, both one tap. A kept card turns green with "Kept · Undo".
    A dropped card fades with "Dropped · Undo". Cards never vanish;
  - when all are decided, one line says "N facts added to your profile. You can undo this for a
    day.", with one Undo for the whole Keep all.
- **Keep** writes the fact on its job, with its quote. A confirmed assumption is marked as such,
  with what it was based on. Keep can also add the skill (forgetting an earlier "not mine"), or
  add the words to the background. Each Keep makes one new profile version, and so does Keep all.
  A fact the profile already holds is closed instead: nothing is written and it is not counted.
- **Undo** (for a day) takes back exactly what Keep wrote. The background goes back only while it
  still ends with the words Keep added. A skill goes with its "not mine" restored. Undo never
  touches anything the profile had before.
- **Profile & goals** shows "N new facts for your profile" with **Check the facts** while facts
  are waiting. Tell me about you's saved screen says in one line how many facts are waiting, with
  the way there.
- The CV frame already has projects, certificates and the background (#73 did it); this change
  does not touch it.

## What the app does without the person, and what still waits on them
The app finds and proposes the facts by itself, with no model and no cost. **Still waits on the
person**: Keep or Drop for each fact, or Keep all. Nothing reaches the profile before that, and
every decision can be undone for a day.

## Cost
No model call. One small read and write per trigger; one profile version per Keep or Keep all.

## Non-goals
- Today's own card for this page: Today's layout and its action values belong to #79 and the
  pipeline module. Today keeps leading to Tell me about you.
- A new model call to read the person's answers again: the answers were read when given; their
  changes are proposed as they are.
- Summaries, headlines, goals, languages and education from Tell me about you: the wizard's Save
  writes those.
- Editing a fact before keeping it: Keep or Drop only, as the owner asked.
