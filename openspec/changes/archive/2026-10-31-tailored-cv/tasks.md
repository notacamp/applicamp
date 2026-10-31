# Tasks
- [x] 1 Kernel: the claim rules (`claims`, `squash`, `Evidence`) move from the profile module, with their tests; skill aliases in `tech`; the profile re-exports
- [x] 2 Profile: the whole profile for a CV, add a fact in the person's own words to a position (a profile version, as Grill me's Keep), add a skill (alias-aware, forgets a dismissal); tests
- [x] 3 Pipeline: migration `cv_drafts`; the frame (sources with ids); the prompt, its schema and the shaping of the answer; the fact check; the views (marks, left out, keywords, changes); endpoints for get, draft, edit/remove, confirm, put back, approve, discard; the two caps; the 7-day sweep; ownership returns 404; unit and HTTP tests (claims pass and fail, unknown source, company fact outside the summary, a skill off the profile, confirm, put back, approve refused while flagged, caps, 404, no contact in the prompt)
- [x] 4 Server wiring of the three new ports
- [x] 5 Client: Draft the CV on Prepare, the review route and page (toolbar, the CV as a page with marks and sources, flagged lines with Edit / It's true / Remove, skills, left out with Put back, the aside: banner, keywords, what changed, spend), the peek's Tailored CV block states; four languages; component tests
- [x] 6 Fakes: the fake model answers the CV prompt (a reworded line, a verbatim one, a wrong number, a company fact as an achievement, a company fact in the summary, a skill off the profile, a role left out) with its own test; e2e happy path
- [x] 7 OpenAPI regenerated, client types regenerated, changelog entry file
- [ ] 8 Gates green in CI, recording (UI at 375px and desktop, by the lead)
