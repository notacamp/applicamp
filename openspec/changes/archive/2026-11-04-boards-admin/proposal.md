# A Job sites page for admins: every board's status, what waits for you, and the permission emails to copy

Issue: https://github.com/notacamp/applicamp/issues/86 (part of the epic #85; the look is the redesign of #78)
Modules: sources (`features/board_requests`, the new `sources.board_requests` table), and the client's sources module (the admin view of `/boards`).

## Why
The app will know thousands of boards, job sites and companies. A person cannot approve them one by one, and the few that do need a
person (a board that wants written permission, a key only the owner can ask for, one choice for a whole hiring system) are lost among the
rest. Asking a board for permission happens outside the app, by email, and nothing records that it was asked or what they said.

## What changes
- **The admin view of `/boards`** for board owners: totals (being read as job sites and companies, waiting for you, being checked, not
  allowed, paused), "This week" (boards found, checked, read again and approved, from what the app recorded), "Waiting for you" (each board
  with a plain reason), and "All boards" (search, filters by kind, status and kind of jobs, a table, a side panel). Everyone else keeps the
  page they have. The previous list stays at `/boards?tab=list`, and "More options" in the side panel opens the full board panel (config, proposals, key).
- **The side panel puts the decision first.** Permission: the email in a copy field, "I sent it" then "They said yes" or "They said no".
  Key: the steps, a field to paste it, "Save and check" (the existing key flow). Reading: Pause, Check again, "Stop reading it" (confirmed in place). Then the
  facts and each check in plain words with the quoted sentence.
- **Permission emails.** `GET /api/v1/boards/{id}/permission-email` drafts the email from a template (the board's name, what Applicamp
  reads, the link back, at most hourly, no reposting, closed jobs dropped), to the board's contact when its check found one. The app never
  sends it.
- **Requests are recorded.** `sources.board_requests` keeps, per board and kind (`permission`, `choice`), when it was asked and the answer.
  "I sent it" stores the date. "They said yes" approves the board through the existing approve path and keeps "They said yes on <date>";
  "They said no" declines it through the existing decline path with that reason. A board's robots rules or a missing key still refuse a yes.
- Endpoints for owners only (anyone else gets 404): `GET /api/v1/boards/admin`, `GET /api/v1/boards/{id}/permission-email`,
  `POST /api/v1/boards/{id}/permission/asked`, `POST /api/v1/boards/{id}/permission/answer`.

## What the app does by itself, and what waits on the person
- By itself: counts every board, works out what each one waits for (permission, key, choice, only an approval), drafts the email, shows what the checks found.
- Waits on the person: sending the email from their own mailbox, recording the answer, pasting a key, the hiring-system choice.

## Reads from the board finder (#75)
The columns it adds (`tags`, `terms_verdict`, `terms_quote`, `terms_url`, `contact_email`, `found_through`, `approved_by`, `approved_at`) and the
reason `needs_permission` are read when present and ignored when not; this change adds none of them.

## Non-goals
- Sending the email, or reading the owner's mailbox for the answer.
- A count of new jobs a week (the data kept for the salary ranking covers only some jobs, so it would say something false).
- Finding or approving boards by itself (#75).
