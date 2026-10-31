# Tasks
- [x] 1 Fact and Area types on the position, profile limits; migration for sessions, turns and proposals; proposals swept with the drafts
- [x] 2 Interviewer prompt, strict schema, context (one role, last 10 turns), versioned
- [x] 3 judge.rs: verbatim quote check, text check against quote and profile, coverage never down, next question rules; unit tests (including bad JSON)
- [x] 4 Endpoints: roles, one role, start, answer, skip, done, keep (Fix), keep-all, drop; daily cap of 150 calls; HTTP tests
- [x] 5 Client: roles list and chat routes, facts block with Keep / Fix / Drop, coverage card, spend line, entry points, kept facts under each role with Remove and Undo; four languages
- [x] 6 Fake model recognises the interviewer; e2e happy path (vague answer, push-back, numeric answer, keep, fix, drop, coverage, facts on the profile)
- [x] 7 OpenAPI regenerated, client types regenerated, changelog entry file
- [ ] 8 Gates green in CI, recording (UI at 375px and desktop, by the lead)
