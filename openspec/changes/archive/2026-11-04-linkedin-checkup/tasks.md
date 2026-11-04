# Tasks
- [x] 1 Migration `profile.linkedin_reviews` and `profile.linkedin_calls`; expiry in `profile.expire_imports`
- [x] 2 Prompt (instructions, schema, message without name or contact details), the LinkedIn profile scrubbed and cut into sources
- [x] 3 Judge: findings, proposals, claim and buzzword flags, edits checked again
- [x] 4 Endpoints: state, check-up (ZIP without a model, PDF through the import's readers), decisions and edits, start over; the daily cap
- [x] 5 Server tests: the flow from the export ZIP, privacy of the prompt, flags, decisions, cap, stale, another account, expiry
- [x] 6 Client: `api/linkedin.ts`, `LinkedInPage` (upload, review, to-do list), `LinkedInSection` on Profile & goals, the route, four languages
- [x] 7 Client tests (page states, keep/edit/drop/done, flagged proposal), e2e with the fake model's review
- [x] 8 OpenAPI regenerated, client types regenerated, changelog entry file
- [x] 9 Gates green in CI, recording (UI at 375px and desktop)
