# Tasks
- [ ] 1 Migration `profile.linkedin_reviews` and `profile.linkedin_calls`; expiry in `profile.expire_imports`
- [ ] 2 Prompt (instructions, schema, message without name or contact details), the LinkedIn profile scrubbed and cut into sources
- [ ] 3 Judge: findings, proposals, claim and buzzword flags, edits checked again
- [ ] 4 Endpoints: state, check-up (ZIP without a model, PDF through the import's readers), decisions and edits, start over; the daily cap
- [ ] 5 Server tests: the flow from the export ZIP, privacy of the prompt, flags, decisions, cap, stale, another account, expiry
- [ ] 6 Client: `api/linkedin.ts`, `LinkedInPage` (upload, review, to-do list), `LinkedInSection` on Profile & goals, the route, four languages
- [ ] 7 Client tests (page states, keep/edit/drop/done, flagged proposal), e2e with the fake model's review
- [ ] 8 OpenAPI regenerated, client types regenerated, changelog entry file
- [ ] 9 Gates green in CI, recording (UI at 375px and desktop)
