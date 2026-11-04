# Design

## Data (schema `profile`)
- `profile.fact_proposals` has these columns:
  - `id` and `account_id`;
  - `target` (`position | skills | background`), and for a position its place in `work` when
    proposed and its key (`grill::role_key`: company, title and start);
  - `text`, `quote`, and `source` (`you_said | you_confirmed | your_website | your_cv`);
  - `basis`: a confirmed assumption's reason, or the page it was read on;
  - `origin` (`assumed_right | assumed_accurate | known | change | cv`): what the fact was found
    in, so what Tell me about you takes back can be closed, and so the page knows which facts
    were marked right;
  - `area` and `state` (`open | kept | dropped`);
  - `fact_id`: what Keep wrote, for Undo. That is the fact's id, `added` for a skill or for
    background words, or `added_undismissed` for a skill whose "not mine" Keep forgot;
  - `batch`: the Keep all a fact was kept with;
  - `created_at` and `decided_at`.
  There is an index on `(account_id, state, created_at)`.
- `profile.markers` (`name`, `done_at`) records one-off work done once per instance (the backfill).
- `profile.imports.applied_at` is when an import was applied: the page's "We read your CV again"
  counts a day from then.
- Kept facts live where they always did: in the profile document (`work[].facts`, `skills`,
  `basics.background`). A fact kept from a proposal has the proposal's id as its id.

## Finding facts (`features/facts/propose.rs`, plain code)
`candidates(profile, version, about, cv)` reads two things: the pending state of Tell me about you
(`profile.about_you`, while it lives) and the import applied last (its draft lives a day).
- An `assumed` sentence is confirmed when the person chose *That's right* (`assumed_right`), or
  chose nothing and answered "Yes, mostly" (`assumed_accurate`). *That's wrong* and *Ask me later*
  are not. A new portrait resets the choices and "Is this accurate?" together.
  - It goes on the job its key names, or on the newest one when it names none, as its claim
    (`claim_of`).
  - When it cites the person's own site (`site…` sources), it is *from your website*, with the
    page as its basis. Otherwise it is *you confirmed*, with its reason.
- A `known` sentence is proposed only when all of these hold:
  - the portrait was made for the current profile version (its sources name jobs by place);
  - the person answered "Yes, mostly";
  - it cites exactly one job (`w{i}`, `w{i}f{j}`), and otherwise only the person's own words
    (`p.summary`, `p.background`). Never the goals, `p.years`, the skills list, the headline,
    education or anything read on a site;
  - it names a number or a tool (`kernel::claims`) that the job's own text does not say.
- A change (`role_fact`, `skill`, `background`) is proposed with its quote in these cases: it has
  no flag; or the person settled its claim flags; or its only claims are ones their quote and
  profile say (`Evidence::of`). A quote that is not their words is never proposed, even after
  *It's true*. *Use my own words* makes their words both the value and the quote. What is left
  out is counted (logged, never shown).
- A CV read again proposes each highlight of a job matched as Merge matches jobs
  (`match_positions`: company and start, then title), unless the job holds it as a highlight or
  a fact.
- Nothing the profile holds is proposed (a fact or highlight of the job, a skill by `same_skill`,
  words of the background), and nothing twice. The key is where the fact goes (a job by its
  company and start) plus its words, ignoring case, spacing and closing punctuation.

`propose_with` stores the new candidates in the caller's transaction. It skips any one already
proposed, in any state, by key, so a dropped fact is never proposed again. It closes (deletes) an
open proposal in two cases:
- the profile now holds it;
- what it came from no longer says it, judged only while that source is there to judge. An
  about-you fact is checked while the pending state lives (one from the profile only while the
  portrait is current, or after "Is this accurate?" stops being "Yes"). A CV fact is checked
  while the applied import lives.
A job that is gone keeps its proposals hidden but not deleted. A job whose title was renamed is
still found by its company and start.

## Jobs and events
- `profile.propose_facts { account }` runs in one transaction, is idempotent and calls no model.
  It is queued in the same transaction as each change to Tell me about you's state (the
  portrait, a choice, "Is this accurate?", "Anything else", an answer, a change settled or
  removed) and as an applied import.
- At Save, Tell me about you first reads the dropped keys and writes none of them, counting them
  as skipped. Then it calls `propose_with` itself, before it deletes its pending state. That call
  compares the portrait's version with the version before Save, because Save does not move the
  positions.
- `profile.backfill_facts {}` is queued at every server start (`enqueue_unique_at`). It does
  nothing once `profile.markers` has `fact_proposals_backfill`. Otherwise it proposes for every
  account with a profile; an account that fails is logged (its id only) and skipped, and then the
  marker is written.
- Keep, Keep all and Undo write the profile through `completion::write_profile`, which publishes
  `profile.updated` (ids only). They write only when something changed. Logs carry counts, never a
  fact.

## Endpoints (session; the person's own rows only; problem+json)
- `GET /api/v1/profile/facts/proposals` returns:
  - groups: jobs newest first, then `skills`, then `background`. Each proposal has its state,
    `said_right` and `undo_until`;
  - counts of `open`, `kept` and `dropped`;
  - `cv_read`: what an import applied in the last day found;
  - `batch`: the last Keep all and how many facts it kept, while it can be undone.
  It lists open proposals and those decided in the last day. A proposal on a job removed since is
  not shown. A kept one whose fact, skill or words were removed by hand since is not shown as
  kept.
- `POST …/proposals/{id}/keep`. Answers: 409 `proposal_not_open`; 409 `profile_changed` when its
  job is gone; 400 `invalid_profile`.
- `POST …/proposals/keep-all` keeps every open proposal in one profile version, under one batch.
  Ones whose job is gone are left as they are. Answers 400 `invalid_profile`.
- `POST …/proposals/{id}/drop`. Dropping twice is fine; dropping a kept one is 409.
- `POST …/proposals/{id}/undo` works within 24 hours of the decision. Otherwise it answers 409
  `undo_expired`, or 409 `proposal_not_decided` when nothing was decided. Also 400
  `invalid_profile`.
- `POST …/proposals/batches/{id}/undo` takes back a whole Keep all in one profile version, within
  24 hours. Otherwise it answers 404, or 409 `undo_expired`.
Another account's proposal is 404.

## AI
None. The `complete` role is not used. The facts come from answers already read, assumptions
already confirmed and files already read.

## Client
`modules/profile`: `api/facts.ts` (query, Keep/Drop/Undo, Keep all; each answer replaces the list
and refreshes the profile and the strength); `components/Complete/CompletePage` (the page, in
`TouchTargets`: 44 px buttons on phones, 36 px on desktop); `components/Complete/FactsNudge` on
Profile & goals and the line on Tell me about you's saved screen; a `DecisionCard` composition in `lib/ui` (green when kept, faded and struck out when
dropped). Route `/profile/complete` (`app/routes/profile-complete.tsx`, registered in the router; Profile &
goals and Tell me about you get one callback each to open it).
There is no sidebar entry; Profile & goals links to it. Four languages, no shortcut hints, a
`data-testid` on every control.
