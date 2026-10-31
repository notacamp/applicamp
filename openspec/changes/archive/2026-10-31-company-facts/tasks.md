# Tasks
- [x] 1 Sources: `Reader::page` (guarded client, same-site redirects, 300 KB cut) and `pages::read_site` (posting, home, five same-site linked pages, robots per host); the board's company website field; unit tests for the link pick, robots and the one-site rule
- [x] 2 Profile: `grill::about` answers a question about a job through Grill me's checks and proposals; a compact profile digest; skill dismissal as a function; tests
- [x] 3 Pipeline: migration (companies, pages, facts, reads, job briefs, questions); site choice; the two prompts and their checks; endpoints for brief, site, read, facts, questions; 30-day cache, posting-keyed analysis; the daily cap of 20 reads; ownership returns 404; HTTP tests
- [x] 4 Server wiring of the new ports; the AI gateway for the pipeline
- [x] 5 Client: Tailored CV block in the peek, the Prepare page (site field, Read with its cost, facts with sources and Wrong remove / Undo, pages line, wants, questions with Answer / I don't have it / Skip and Grill me's Keep / Fix / Drop, spend line, plain errors, a spot for the draft); four languages
- [x] 6 Fakes: the fake company site (robots.txt, linked pages, a forbidden page), the fake model for the three prompts; e2e happy path
- [x] 7 OpenAPI regenerated, client types regenerated, changelog entry file
- [ ] 8 Gates green in CI, recording (UI at 375px and desktop, by the lead)
