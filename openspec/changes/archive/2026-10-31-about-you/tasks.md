# Tasks
- [x] 1 Profile: `basics.background`, fact `status` and `basis`; migration for `about_you` and `about_calls`; pending state swept with the drafts
- [x] 2 Port `EmployerResearch` (sourced needs a quote and its page); pipeline company read split so a past employer and a job's company share it; server wires the port
- [x] 3 Portrait and "Anything else" prompts, strict schemas, versioned; `judge.rs`: downgrade not drop, flags not drops, own words
- [x] 4 Endpoints: state, portrait, choose, accuracy, employer site, extra, settle and remove a change, review, atomic save; daily cap of 30 calls; HTTP tests
- [x] 5 Client: `/profile/about` (portrait, accuracy, anything else, added, review, saved), `Portrait` composition, entry on the strength card, background and confirmed assumptions on the profile; four languages
- [x] 6 Fake model recognises the portrait and the text reader; employer site is #34's fake company site; e2e happy path
- [x] 7 OpenAPI regenerated, client types regenerated, changelog entry file
- [ ] 8 Gates green in CI, recording (UI at 375px and desktop, by the lead)
