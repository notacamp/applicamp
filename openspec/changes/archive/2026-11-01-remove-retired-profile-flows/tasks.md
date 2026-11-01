# Tasks
- [x] 1 Remove the question, draft and suggested-skills endpoints and the `profile.gap_review` job (nothing enqueues it)
- [x] 2 Remove the Grill me interview endpoints (roles list, start, answer, skip, done) and its prompt, keeping the proposals and their controls
- [x] 3 Migration dropping `profile.gap_reviews`, `profile.drafts` and `profile.grill_turns`, and deleting queued `profile.gap_review` jobs; `skill_dismissals`, `grill_sessions` and `grill_proposals` stay
- [x] 4 Tests of the removed code deleted; the proposal tests (keep, fix, drop, keep all, expiry, other account) rewritten to seed proposals directly and still pass
- [x] 5 Fake model: branches for the gap review, the draft and the interviewer removed
- [x] 6 OpenAPI and client types regenerated; changelog entry file
- [x] 7 Spec deltas; change archived
- [ ] 8 Gates green in CI
