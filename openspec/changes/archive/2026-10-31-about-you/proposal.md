# Tell me about you, part 1: what Applicamp knows and assumes, confirmed by you

Issue: https://github.com/notacamp/applicamp/issues/43 (part 2 is #44: the questions about the gaps, which retires Improve my profile, Skills and Grill me as separate flows)

## Why
The current profile flows do not feel like what the owner asked for. The strict
check silently dropped a summary answer twice, and Grill me's first question
was "What does the company sell, and who are its customers?", which "could be
figured out with a simple search online". The ask was a wizard: here is what I
know about you, in a text; is it accurate; is there anything else I should know;
and every question justified by what I assumed or don't know. It should
suggest what it can assume from knowing the person's role. Prototype approved:
https://claude.ai/artifact/FXuFikB8ZXieD43uGB6xxH (first and last screens are
this change).

## What changes
- **A portrait**, the first screen: "Here's what I know about you". One written
  text from the profile (without name, email, phone, address or links), the
  facts and goals the person kept, and what the latest five past employers say
  about themselves on their own websites. Every sentence is marked by where it
  comes from: from your profile, you told me, or I assumed. Assumptions include
  **role-typical suggestions** ("Coliving platforms are usually bookings,
  payments and building access, so I assume you worked on some of those") and
  each carries its reason.
- **Settle each assumption**: *That's right* (a confirmed fact in the profile,
  written at Save), *That's wrong* (struck out, never saved, not suggested again
  for this profile version) or *Ask me later* (kept for part 2's questions).
- **No silent drops.** A sentence the model calls from your profile or from what
  you told us, which the claim check cannot find in the sources it cites, is
  **downgraded to an assumption**, never dropped.
- **Is this accurate?** (yes mostly / some of it's wrong / not really), stored for
  part 2, and **Anything else I should know about you?**: free text of up to
  4,000 characters becomes proposed changes across the profile: summary, headline,
  skills (matched to existing ones and aliases), background (a new profile field),
  goals, and facts on a named position. Each change quotes the person's words.
  A claim their words do not say is a **flag, shown and never dropped**: *It's
  true*, *Fix*, or *Use my own words*, which puts their exact text in the field.
  The new sentences join the portrait as *told*, highlighted. Nothing is saved yet.
- **Review and save**: every pending line grouped by part of the profile
  (Summary, Headline, Skills, each position, Background, Goals), each marked *You
  told me* or *Confirmed*, with Edit (re-checked) and Remove. **Save** writes
  everything in one profile write: one new profile version, so the strength and
  the gap review refresh. Assumptions never confirmed are not saved.
- **Employers are read through a port**, not copied: the pipeline's company facts
  (site choice, page reader with robots and host guard, quote-on-page check,
  30-day per-account cache) serve a past employer as they serve a job's company.
  No loop, no open-web search; an employer with no known site is not fetched.
- **Entry**: a "Tell me about you" button on Profile & goals takes the place of
  "Improve my profile" on the strength card. The Improve route stays (part 2
  retires it).
- Route `/profile/about`.

## Modules touched
`profile` (server: new slice `features/about`, schema `profile`, a port
`ports::EmployerResearch`; a `background` field and a fact status on the profile;
client: `modules/profile`, a route, a `Portrait` composition in `lib/ui`),
`pipeline` (the company read is refactored so a job's company and a past employer
share it; a small `employer` entry point), `server` (wires the port), the fake
model in `clients/app/scripts/fake-jobs.ts`.

## Cost
AI role `complete`, temperature 0, no background calls, no loop.

| Step | Calls | Estimate |
|---|---|---|
| Portrait | 1 per profile version | about EUR 0.002 |
| Employer research | 1 per employer, at most 5, cached 30 days, fetch only when the site is known | about EUR 0.002 each |
| Anything else | 1 per submit | about EUR 0.001 to 0.002 |

A first run is at most about EUR 0.014 (5 employers, the portrait, one text).
Settling assumptions, editing, removing, reviewing and saving make no call;
opening the page again for the same profile version makes none either.

Every call goes through the existing budget, which reserves its worst case
before it is sent: EUR 0.30 a day and EUR 5 a month per account. Over a limit
the call is refused before it is sent and the page says so in plain words
(`ai_budget_reached`). A new hard cap on top: **30 wizard calls per account per
UTC day** (`about_daily_cap`, HTTP 429), counted from the wizard's own call
log; an employer read counts as one. Employer reads also count toward the
pipeline's existing 20 company reads a day.

## Non-goals
- Part 2 (#44): questions about the gaps, each justified, and retiring Improve my
  profile, Skills and Grill me. Part 1 ends at "Questions about the gaps come next."
- A loop, an agent, or any open-web search. The harness's `profile_research` run
  kind will implement the same port later; the wizard does not change.
- Any call in the background; any change to what Grill me or Improve ask.
- Reading a site that is not known (no guessing a company's website).
- Voice or file input: text only.
