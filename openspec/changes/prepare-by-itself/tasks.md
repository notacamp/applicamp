# Tasks
- [x] 1 Migration `pipeline.preparations`; new `SiteSource` values
- [x] 2 Website finder: posting links and email domains, name guesses through `names_company`; unit tests (Perry Street-shaped posting, webmail, a tracker link, two candidate domains)
- [x] 3 `Neighbours::autonomy` and `names_company`, implemented in the server
- [x] 4 The `pipeline.prepare` job: read → stop on open questions → draft; letter after approval; guard row; caps and budget stop it with a reason; tests (shortlist → draft; questions wait; Find only does nothing; Stop; back to Found; cap)
- [x] 5 Triggers: stage PATCH, opening the brief, keeping the last answer, approving a CV
- [x] 6 "Draft the CV" reads the company first when it can
- [x] 7 Endpoints `prepare/stop` and `prepare/start`; brief GET carries `preparation` and the site's source
- [x] 8 Client: Prepare shows progress, where the site came from, "done by itself · €…", Stop / Prepare by itself / Try again; peek and Today show what waits on you; i18n en, pt-PT, es-ES, fr
- [ ] 9 OpenAPI regenerated, client types regenerated, changelog entry file (changelog/unreleased/added/70-prepare-by-itself.md), gates green in CI, recording (UI at 375px and desktop)
