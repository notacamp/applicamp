# Tasks
- [x] 1 config: `skill_search` (validated), `stack_queries`, top skills; ttl rule for `max_fetches_per_day`
- [x] 2 reader: stack queries merged and deduplicated, failures keep the plain read
- [x] 3 matching: `MatchInput.skills`, skill words, pre-rank bonus; server wiring
- [x] 4 `board_jobs.words` migration; fit computed for the Boards list and peek
- [x] 5 seed list: `skill_search` for Jobicy, RemoteOK, Himalayas; five stack candidates
- [x] 6 client: "fits your skills N%" in the list and the peek; 4 languages; e2e + unit tests
- [x] 7 tests: query mapping, merged reads, pre-rank, fit, seed list
- [ ] 8 OpenAPI regenerated, client types regenerated, changelog entry file, gates green in CI, recording (UI at 375px and desktop, by the lead)
