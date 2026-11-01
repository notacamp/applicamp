# Remove the retired Improve, draft, suggested-skills and Grill me interview endpoints

Issue: https://github.com/notacamp/applicamp/issues/49 (follow-up to #44, which retired the flows from the client)

## Why
#44 retired Improve my profile, the Skills page and Grill me from the client, but left
their server endpoints, their background job and their tables in place, deprecated and
called by nothing. Dead endpoints still cost: they are public API surface to keep safe,
a background job that can spend model money, and tables nobody reads.

## What changes
- **Removed endpoints:** `GET /api/v1/profile/questions`, `POST .../questions/review`,
  `POST .../questions/{id}/draft`, `POST .../questions/{id}/skip`,
  `POST .../drafts/{id}/keep|discard`, `GET .../skills/suggested`,
  `POST .../skills/suggested/{name}/add|dismiss`, `GET .../grill` (the roles list) and
  `POST .../grill/{position}/start|answer|skip|done`.
- **Removed job:** `profile.gap_review` (nothing enqueues it any more). Reviews still
  waiting in the queue are deleted by the migration.
- **Dropped tables:** `profile.gap_reviews`, `profile.drafts`, `profile.grill_turns`.
- **Kept:** the facts taken from an answer about a job and their controls
  (`GET /grill/{position}`, `POST /grill/proposals/{id}/keep|drop`,
  `POST /grill/{position}/keep-all`, tables `grill_sessions` and `grill_proposals`),
  because a tailored CV's question and Tell me about you's answers still end as facts
  on a position; `PUT /skills/order`; `GET /strength`; `profile.skill_dismissals`
  (Tell me about you's questions read it).
- **Unchanged on the wire:** the response types the client still reads (`StrengthView`'s
  `questions_left` and `suggested_skills`, `GrillView`'s interview fields) stay, always
  none or empty, so the client needs no change; trimming them is a client follow-up.

## Impact
No UI changes. The API loses the endpoints above; no client called them.
