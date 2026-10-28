# Tasks
- [x] 1 registry: `blockers`, `api_key*` columns; `Blocker`, `Proposal`, `KeySpec`, `link_base`, `fields.work_mode`; `from_json` and validation naming the field
- [x] 2 seed list: SwissDevJobs, Net-Empregos, The Muse configs; ITJobs.pt and Adzuna keys; Landing.jobs, Wellfound, Dice blockers; nothing declined; sync refreshes blockers and notes
- [x] 3 migration: seeded declines wait as candidates with notes and blockers
- [x] 4 keys: `board_keys` (sealed, bound to the board), `PUT`/`DELETE …/key`, reader sends the key in the query only
- [x] 5 approval: `blockers()` in order, `BoardDetail.blockers` and `key`, 409 with the first blocker's detail
- [x] 6 check: own config first, probes and sitemap when none reads, `report.proposal`, `PUT …/config` with `kind`; `sources.check_seeded_boards` at start
- [x] 7 client: blockers, per-action errors with detail, key field, proposal, "Advanced"; 4 languages
- [x] 8 tests: seeded candidate approvable after its check; `needs_key` then key set then approvable; config errors name the field; declined → candidate migration; proposal from an RSS link and from a sitemap; e2e desktop + phone
- [ ] 9 OpenAPI regenerated, client types regenerated, changelog entry file, gates green in CI, recording (UI at 375px and desktop, by the lead)
