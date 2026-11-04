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
  - from Tell me about you: each assumption the person confirmed. That is *That's right*, or no
    answer when they said the portrait is accurate. It also proposes each portrait sentence taken
    from the profile that names a number or a tool its job does not hold yet (only when they said
    the portrait is accurate and it was made for this same profile). And it proposes what they wrote
    under "Anything else" or in an answer, as a fact on a job, a skill or their background. A change
    whose claim check failed, and that the person did not settle, is left out and counted, never
    shown;
  - from their own website: what the portrait assumed from it, confirmed the same way, with the
    page it was read on;
  - from a CV read again: an achievement its job does not hold yet.
  The app never proposes the same fact twice, never one the person dropped, and never one the
  profile already holds.
- **When**: the job `profile.propose_facts` (ids only, free) runs in these cases: when the
  portrait is made (the own website is read with it); when an assumption is marked right; when the
  portrait is said to be accurate; when "Anything else" or an answer is read; when a flag is
  settled; when an import is applied. At Save, Tell me about you proposes what it does not write
  itself, in the same transaction, before its pending state is deleted. Once per instance,
  `profile.backfill_facts` does the same for the accounts that existed before this change. A
  marker stops it from running twice.
- **A page, "Complete your profile"** (`/profile/complete`): "We read your CV again. Found 13 jobs…"
  when an import was applied in the last day; a progress line "N of M decided" with a bar and
  **Keep all**; groups by job ("Example Co · 2021 – now"), then skills, then background. One card per
  fact shows the text, a small tag for where it comes from (*you said*, *you confirmed*, *from your
  website*, *from your CV*) and the words it comes from. Each card has **Keep** and **Drop**, both
  one tap. A kept card turns green with "Kept · Undo". A dropped card fades with "Dropped · Undo".
  Cards never vanish. When all are decided, one line says "N facts added to your profile. You can
  undo this for a day."
- **Keep** writes the fact on its job, with its quote. A confirmed assumption is marked as such,
  with what it was based on. Keep can also add the skill, or the words to the background. Each Keep
  makes one new profile version, and so does Keep all. **Undo** (for a day) takes back exactly what
  Keep wrote, and nothing the profile had before.
- **Profile & goals** shows "N new facts for your profile" with **Check the facts** while facts
  wait.
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
