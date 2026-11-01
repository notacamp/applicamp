# Design

The `profile` module owns the wizard's data (`profile.about_you`); the employer facts stay
the `pipeline` module's (`pipeline.company_facts`), reached through the existing
`EmployerResearch` port. No schema change and no event. The new endpoint needs a
signed-in session (no scope: it is the person's own profile) and works only on that
account's row. Errors are problem+json.

## Researching the employers (`features/about/mod.rs`)
`research_employers` replaces the sequential loop. It plans first: an employer with a known
site that did not fail last time is read, up to what the wizard's day limit leaves (the
portrait's own call kept in reserve); the rest are marked failed with `about_daily_cap`.
The reads run as a stream with `buffer_unordered(3)`, each wrapped in a 25 second
`timeout`, the whole stream under a 28 second deadline. Each result is applied to its
employer: read (facts counted) or failed with the error's code (`site_unreachable`,
`robots_disallows`, `no_pages_readable`, `ai_failed`, `research_timeout`, ...). Employers
still running at the deadline are dropped and marked `research_timeout`. A missing model
or a spending limit (`ai_unavailable`, `ai_daily_limit_reached`, `ai_budget_reached`) is the
one failure that still ends the step, in plain words: nothing else could be called.
Every read that started counts as a `research` call in `profile.about_calls`, whatever came of
it, so a failure cannot be hidden from the limit.

An employer whose stored state is `failed` is skipped by the portrait (its site is known, its
last read failed); only Try again, or typing a new website (which resets it to `unread`),
reads it. `employers_of` already carried the stored failure forward.

The portrait call is wrapped in a timeout of what is left of 40 seconds from the request's
start, never less than 12; on timeout it is `ai_failed`, so the answer comes within about 40
seconds plus the 12 second floor in the worst case, never a minute of silence.

## Try again (`POST /api/v1/profile/about/employers/{position}/retry`)
Needs the portrait to exist (409 `portrait_needed`), an employer at that position with a
known site (404 otherwise), and room under the day's limit (429 `about_daily_cap`). It runs
the same single read with the same 25 second timeout, counts it, and writes the result into
that employer's entry of the stored row (read with its facts, or failed with the reason). It
returns the wizard's state; it does not remake the portrait.

## The facts call (`pipeline/features/brief`)
`prompt::pages_budget(attempt)` is 16 000 characters for the first attempt and 8 000 for the
second; `facts_message_within` cuts the pages (posting first, then in order) to a budget. The
shared `ask` now sits on `ask_attempt`, one call with a fixed answer size; the facts call runs
two attempts at 4 000 tokens each, the second with half the text. The schema carries `maxItems`
10 and `maxLength` 160 and 200; the judge keeps at most 10. `FACTS_VERSION` goes to 2.
The log says which attempt worked, and the number of pages, facts and dropped ones; never text.

## The question call (`features/about/prompt.rs`, `questions.rs`)
`EmployerResearch::known_facts` (a default method returning nothing, wired to a read of
`pipeline.company_facts` with no fetch and no call) gives the position's employer's facts, at
most 8 of 200 characters, shown in `<company_facts>`. Nothing else is added to what the model
is shown: still no contact details and no other position. `QUESTION_INSTRUCTIONS` is rewritten
(plain words, start from what is known, one sentence, a plain `why`, example outcomes for a
results gap, never ask what the context answers); `PROMPT_VERSION` goes to 3. The model is told
not to add "Something else": the answer box is always there, labelled "Something else? Say it in
your own words" when choices are shown. `gaps::results_basis` is the plain fallback `why`.

## The answer
Unchanged in structure: picked choices are part of the person's words, so a pick alone is a
valid, quoted answer. `ANSWER_INSTRUCTIONS` says a pick becomes a short fact that says only what
the pick says, with no number. The client treats a results question as multi-select.

## Client
`PortraitStep` shows, for a failed employer, `about-employer-failed-<position>` (the message and
the reason, `about.employers.reason.<code>`) and `about-employer-retry-<position>`. The question
card shows a hint under the choices for a results question (`about-q-number-hint`).

## AI use
Role `complete`; without it the wizard says so, as before. The person can skip any question, and
nothing is saved until Save. Undo is unchanged: pending changes can be removed before Save.
