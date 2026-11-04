# Design

## Data (schema `profile`)
- `profile.fact_proposals`: `id`, `account_id`, `target` (`position | skills | background`), for a
  position its place in `work` when proposed and its key (`grill::role_key`: company, title and
  start, so a list edited since never lends a fact to another job), `text`, `quote`, `source`
  (`you_said | you_confirmed | your_website | your_cv`), `basis` (a confirmed assumption's reason,
  or the page it was read on), `area`, `state` (`open | kept | dropped`), `fact_id` (what Keep wrote,
  for Undo: the fact's id, or `added` for a skill or background words), `created_at`,
  `decided_at`. Index on `(account_id, state, created_at)`.
- `profile.markers` (`name`, `done_at`): one-off work done once per instance (the backfill).
- Kept facts live where they always did: the profile document (`work[].facts`, `skills`,
  `basics.background`). A fact kept from a proposal has the proposal's id as its id.

## Finding facts (`features/facts/propose.rs`, plain code)
`candidates(profile, version, about, cv)` reads the pending state of Tell me about you
(`profile.about_you`, while it lives) and the import applied last (`profile.imports`, while its
draft lives, a day):
- An `assumed` sentence is confirmed when the person chose *That's right*, or chose nothing and
  answered "Yes, mostly". *That's wrong* and *Ask me later* are not. It goes on the position its key
  names, or on the first one when it names none (as Save places it), as its claim (`claim_of`).
  When it cites the person's own site (`site…` sources), it is *from your website*, with the page as
  its basis. Otherwise it is *you confirmed*, with its reason.
- A `known` sentence is proposed only when the portrait was made for the current profile version
  (its sources name positions by place), the person answered "Yes, mostly", it cites one position
  (`w{i}`, `w{i}f{j}`), and it names a number or a tool (`kernel::claims`) that the position's own
  text does not say.
- A change (`role_fact`, `skill`, `background`) is proposed when it has no flag, when the person
  settled its flag, or when its only flags are claims that their quote and profile say
  (`Evidence::of`). A quote that is not theirs is never proposed. What is left out is counted
  (logged, never shown).
- A CV read again: each highlight of a position the profile has (the merge rule `same_position`)
  that the position holds neither as a highlight nor as a fact.
- Nothing the profile holds (a fact or highlight of the position, a skill by `same_skill`, words of
  the background) and nothing twice.

`propose_with` stores the new ones in the caller's transaction. It skips any one already proposed,
in any state, by place and words, so a dropped fact is never proposed again. It also deletes open
proposals that the profile now holds (Save wrote them, or the person added them by hand).

## Jobs and events
- `profile.propose_facts { account }`: one transaction, idempotent, no model. It is queued in the
  same transaction as: the portrait, *That's right*, "Yes, mostly", "Anything else", an answer, a
  settled flag, an applied import. At Save, Tell me about you calls `propose_with` itself, before it
  deletes its pending state. It compares the portrait's version with the version before Save, whose
  positions Save does not move.
- `profile.backfill_facts {}`: queued at every server start (`enqueue_unique_at`); does nothing once
  `profile.markers` has `fact_proposals_backfill`, else proposes for every account with a profile
  and writes the marker.
- Keep, Keep all and Undo write the profile through `completion::write_profile`, which publishes
  `profile.updated` (ids only). Logs carry counts, never a fact.

## Endpoints (session; the person's own rows only; problem+json)
- `GET /api/v1/profile/facts/proposals`: groups (positions newest first, then `skills`, then
  `background`), each proposal with its state and `undo_until`, counts `open | kept | dropped`, and
  `cv_read` (what an import applied in the last day found). It lists open proposals and those
  decided in the last day. A proposal on a job removed or renamed since is not shown. A kept one
  whose fact, skill or words were removed by hand since is not shown as kept.
- `POST …/proposals/{id}/keep`: 409 `proposal_not_open`, or `profile_changed` when its job is gone.
- `POST …/proposals/keep-all`: every open one, one profile version; ones whose job is gone are left
  as they are.
- `POST …/proposals/{id}/drop`: dropping twice is fine; dropping a kept one is 409.
- `POST …/proposals/{id}/undo`: within 24 hours of the decision (409 `undo_expired` after,
  `proposal_not_decided` before). Undo takes back only what Keep wrote.
Another account's proposal is 404.

## AI
None. The `complete` role is not used. The facts come from answers already read, assumptions
already confirmed and files already read.

## Client
`modules/profile`: `api/facts.ts` (query, Keep/Drop/Undo, Keep all; each answer replaces the list
and refreshes the profile and the strength); `components/Complete/CompletePage` (the page, in
`TouchTargets`: 44 px buttons on phones, 36 px on desktop); `components/Complete/FactsNudge` on
Profile & goals; a `DecisionCard` composition in `lib/ui` (green when kept, faded and struck out when
dropped). Route `/profile/complete` (`app/routes/profile-complete.tsx`, registered in the router).
There is no sidebar entry; Profile & goals links to it. Four languages, no shortcut hints, a
`data-testid` on every control.
