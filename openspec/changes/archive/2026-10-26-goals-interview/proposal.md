# Goals: what you want next, asked one question at a time

Issue: https://github.com/notacamp/applicamp/issues/4

## Why
Searches, scores and CVs need to know what the person wants, not only what
they did. A short interview captures it in structured answers agents can use.

## What changes
- Goals stored per account as typed answers with a version number (agents
  record which version they used).
- A first-run interview: one question per screen, every question skippable,
  leave at any time and come back (never forced).
- The Goals section of Profile & goals shows every answer as an editable
  property.

## Modules touched
`profile` (goals live with the profile). Client: `modules/profile`.

## Non-goals
- Using the goals in searches (issue #5).
- Model-written questions: the questions are fixed and translated.
