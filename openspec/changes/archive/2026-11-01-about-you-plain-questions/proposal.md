# Tell me about you asks in plain words with suggestions, and one unreadable employer no longer stops the portrait

Issue: https://github.com/notacamp/applicamp/issues/51 (builds on #43 and #44, the wizard, and #34, the company facts it reuses)

## Why
Two things the owner hit on prod:

1. **"Update the portrait isn't working."** The employer-research call (the company
   facts read) was cut off at 4000 and again at 8000 tokens for one employer, and
   that one failure failed the whole portrait: a 503 after 44 to 74 seconds. The
   other employers had read fine.
2. **A question the owner could not read:** "What result did you achieve as Full Stack
   Developer at <employer>, with a number, and how do you know it?" ("wtf what does
   this mean"). It came from the results gap, worded literally, without the context
   the app already holds about that employer and role, and with no way in except an
   empty box.

## What changes
- **Employer research is best effort.** At most three employers are read at a time,
  each within about 25 seconds and all within about 28 seconds. A read that fails
  (cut off, too slow, site unreachable, robots.txt, an AI failure) is recorded on that
  employer only; the portrait is made from the profile, the facts and the employers
  that read. The whole portrait request answers, with the portrait or its error,
  within about 40 seconds. A failed read is not retried by itself, not even when the
  portrait is made again.
- **The employer's row says so and offers Try again.** "Couldn't read its website this
  time", with the reason (its robots.txt, it didn't answer, it took too long, the AI
  couldn't read it), and a **Try again** button that reads only that employer
  (`POST /api/v1/profile/about/employers/{position}/retry`). It counts against the
  wizard's daily limit like any read. Typing a new website forgets the failure.
- **The company-facts call fits its budget** (shared with #34): at most 10 facts (was
  12), each text about 160 characters and each quote about 200, in the prompt and in
  the schema; the page text it is shown drops from 24 000 to 16 000 characters; when
  an answer is cut off the one retry is shown **half** the text (the posting first,
  then the pages by priority) at the same answer size instead of the same text with
  more room. Two attempts in all, and the log says which one worked (counts only).
- **Questions in plain words, with suggestions.** The question call also sees the
  facts already kept about the position's employer (what it sells, to whom). The
  prompt says: everyday words like a friendly recruiter, start from what is known or
  assumed, one short sentence, a plain `why`, and never ask what the context already
  answers. For a results gap it asks what changed because of the person's work and
  offers 2 to 4 concrete example outcomes that fit the company and the role as
  choices; a hint says a rough number helps but may be skipped. The gap's own fallback
  reason is plain too ("Your role at <employer> says what you did, but not what
  changed because of it.").
- **Picking is an answer.** Picking choices without writing anything is valid; each
  pick becomes a fact in the person's pick, with no number added. A number they type
  is kept as given.

## Modules touched
`profile` (`features/about`: portrait research, the retry endpoint, gaps, questions,
prompt; the port gains `known_facts`; client `modules/profile`), `pipeline`
(`features/brief`: the facts call), `server` (the port's wiring), the fake model and
fake sites in `clients/app/scripts/fake-jobs.ts`, the e2e stack alias list.

## Cost
AI role `complete`, temperature 0, no background calls, no loop. Nothing is added on the
happy path; a few calls are different:

| Step | Calls | Estimate |
|---|---|---|
| A past employer's read | 1 per employer, as before; now smaller (16 000 characters of page text, at most 10 short facts) | about EUR 0.002 or a little less |
| A cut-off read | at most 1 more, with half the text (before: 1 more with the same text and double the answer room) | about EUR 0.001 more |
| Try again on a failed employer | 1 small call, only when the person asks | about EUR 0.002 |
| A question | 1 as before; the prompt is slightly longer (employer facts, at most 8 short lines) | about EUR 0.001 |

A failed read is still charged by the gateway and counts toward the wizard's 60 calls a
day and the company-read cap of 20; it is never retried by itself, so one failure cannot
multiply the spend. Every call goes through the existing budget, which reserves the
worst case before the call.

## Non-goals
- Retrying failed reads automatically, or reading employers in the background.
- Reading anything but the employer's own site, or searching the web for it.
- Changing what the app asks about (the ranked gaps) or how the profile is saved.
- Re-making the portrait when a retry succeeds: the person updates it, and the facts
  are cached.
