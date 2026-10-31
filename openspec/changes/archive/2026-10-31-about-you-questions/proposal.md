# Tell me about you, part 2: the questions about the gaps, each saying why

Issue: https://github.com/notacamp/applicamp/issues/44 (part 1 is #43, merged: the portrait, the assumptions, "anything else", the review and Save)

## Why
The owner asked for a wizard that "asks about gaps in knowledge, one by one", and
where "in every question it should justify the question telling me what it assumed
or doesn't know about me", suggesting "some stuff it can assume from knowing about
my role". Improve my profile, "Draft from your answer", the Skills page and Grill
me did not feel like that ("this doesn't feel like what I asked"), and Grill me's
first question, "What does RemoteLock sell, and who are its customers?", is the
example of what must never be asked: it "could be figured out with a simple search
online". Part 1 ends at "Questions about the gaps come next." This part is those
questions, and it retires the four flows they replace.

## What changes
- **What is missing is decided in code, with no AI.** A ranked list of gaps is
  computed from the profile, the kept facts, the confirmed and wrong assumptions,
  the goals and the wizard's state: assumptions the person left for later (and,
  after "Not really", those they called wrong); the latest role's results without
  a number; empty skills; a lead or manager title without a team size; results
  without a number on the other two recent roles; technologies the roles name that
  are not skills ("Your roles mention Kafka, but it isn't a skill"); a skill the
  person told that none of their roles mention ("You mentioned Elixir, but none of
  your roles do"); an empty summary, headline or background; goals not answered;
  and the areas of each recent role nothing has been kept about (your part, stack
  and scale, hard problems, stories, people). Context (what the company sells, to
  whom) is **never** asked: it comes from the employer's own website or from an
  assumption to confirm. Questions stop when the list is empty, at 12 (8 after
  "Yes, mostly", 16 after "Not really"), or when the person says "That's enough
  for now".
- **The model only words the question.** One small call per question, shown only
  the top gap and its context (that position's entry and facts, the related
  assumption), never the whole profile and never contact details. It returns the
  question, a `why` that says what was assumed or what is not known ("Your title
  was Lead Engineer, which usually means leading 3 to 6 engineers. Your profile
  doesn't say, and recruiters ask."), a tag (**I assumed**, **I don't know**,
  **You told me**) and choices only for closed questions. The prompt forbids
  asking for anything the model knows or the employer's site covers; it states it
  as an assumption to confirm instead.
- **An answer is one small call** that turns the person's words (and the choices
  they picked) into proposed changes with the same structure and checks as
  "Anything else": a quote that is theirs, a value that says only what they said;
  what fails is **flagged, never dropped** (It's true, Fix, Use my own words). The
  changes join the pending ones and the portrait gains "told" sentences marked
  new. Nothing is saved until Review, then Save.
- **Skip** marks the gap skipped for this profile version; **That's enough for
  now** goes to the review. Each question shows the profile's strength and "→ N%
  if you answer", computed by the server from the gap's weight in the strength
  (a result with a number on a recent role, skills, a summary, a headline, a goal).
  A kept fact about a position's results now counts like a highlight in the
  strength, so the number is true after Save.
- **The wizard moves into the questions** after "anything else" (or straight from
  the portrait when nothing was written), and into the review when the questions
  end.
- **Retired**: the Improve my profile page and its drafts ("Draft from your
  answer"), the Skills page, and Grill me. `/profile/improve`, `/profile/skills`
  and `/profile/grill*` redirect to `/profile/about`. The "Grill me" button, the
  per-role "Grill me about this role" links, the Today nudge's links and the
  strength checklist's Add and Improve links all go to Tell me about you. The kept
  facts stay on the profile ("What you told Applicamp about each role"). Skills
  are reordered from the profile's own skills list. The server endpoints are
  deprecated, not deleted (see design).

## Modules touched
`profile` (server: `features/about` gains `gaps` and `questions`; the strength
counts results facts; client: `modules/profile` and the routes), the fake model in
`clients/app/scripts/fake-jobs.ts`, the e2e tests.

## Cost
AI role `complete`, temperature 0, no background calls, no loop.

| Step | Calls | Estimate |
|---|---|---|
| A question | 1 per question shown (none when it is already on screen, skipped, or the list is empty) | about EUR 0.001 |
| An answer | 1 per answer | about EUR 0.001 to 0.002 |

At most 12 questions in a normal run: about EUR 0.03 on top of part 1's EUR 0.014.
Skipping, "That's enough for now", reloading and reviewing make no call.

Every call goes through the existing budget (EUR 0.30 a day and EUR 5 a month per
account), which reserves the worst case before the call. The wizard's own cap
counts these calls too and is **raised from 30 to 60 per account per UTC day**: a
full run is a portrait, up to five employer reads, "Anything else", and up to 16
questions with an answer each (39 calls), which 30 would not allow. The
Improve review (a background read of every profile version, fired by the strength
card) is gone from the client, so the app makes one less automatic call.

## Non-goals
- Deleting the retired server endpoints and their tables (Grill me's interview and
  proposals are still how an answer about a job becomes facts; the Improve review
  and drafts have no caller left): a follow-up issue.
- Letting the model decide what to ask, or ask about anything not in the ranked gaps.
- Reading the open web for a company, or asking what can be looked up.
- Any call in the background; voice or file input.
