# 0010 — The app does the work first; you approve and stay in control

**Status:** Accepted · 2026-11-03 (#68) · replaces the "every step starts with a click" rule in the pipeline and profile specs

## Context
The pipeline and profile specs say that opening a page makes no model call and
fetches nothing, that every step starts with a click, and that nothing runs in
the background. In practice that leaves the person the work: they type a
company's website the posting already hints at, click four times to prepare
one job, and type a role and company a fetched page already holds. The
Autonomy goal ("find only", "prepare", "prepare and schedule") is collected
but nothing reads it.

## Decision
- **If the app can do a step without you, it does it** and shows you the
  result to approve or correct. An empty field the app could fill, a button
  for a step whose inputs are ready, and copying text between screens are
  defects, unless the proposal of the change says why the step needs you.
- **Approvals stay.** Submitting an application and sending an email still
  need an accepted approval (0003, 0009). Nothing reaches the profile, and no
  CV, letter or answer is ready to send, until you accept it. The fact-check
  gate still runs. Moving an opportunity's stage is proposed, not done for you.
- **You stay in control.**
  - Every automatic step says what it did, from what, and what it cost.
  - A guess is shown as a guess, with where it came from (for example "from
    the posting's email address"), until you confirm it.
  - Your correction always wins and is never overwritten by a later guess.
  - You can stop work in progress, undo what it made, and turn automation
    down. The Autonomy goal sets how far it goes on its own; changing it
    stops queued work at its next step.
- **Limits stay.** The AI budget reserves before every call, daily caps
  apply, robots.txt and the sources' terms (0006) are honoured, and web and
  email content stays untrusted data. When a cap or the budget stops
  automatic work, the page says so and offers the step as a click.
- **When it runs.** A cheap step whose inputs are ready may start when the
  page opens or when the event that makes it possible happens (for example a
  job is shortlisted). Background work is a standing order that starts a
  fresh, bounded run (0009), never a sandbox kept alive.

## Alternatives
- **Keep click-first and add a "do it all" button:** still a click per job,
  and the person has to know when to press it.
- **Fully automatic, no approvals:** a wrong application or email goes out
  under the person's name (0003).

## Consequences
Every spec line that says a step "SHALL start with a click" or that nothing
runs in the background changes in its own OpenSpec change; until then the
spec stands. Automatic work spends the person's AI budget without a click, so
caps and the budget shown on each page matter more. The follow-ups found in
the audit are listed on #68.
