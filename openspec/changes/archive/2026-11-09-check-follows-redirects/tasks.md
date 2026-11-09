# Tasks
- [x] 1 `readers`: a check's read sees a redirect (`FetchError::Redirect`, only from `visit_noting`); store and search reads unchanged
- [x] 2 `Visits::get_final` follows same-site redirects (3 hops, budget, address check, robots), remembers a move to another site; `get` and friends wrap it
- [x] 3 The check goes on from the final homepage (robots for its host, links, terms, finder); proposed configs use final feed, API and sitemap addresses
- [x] 4 Report `moved_to` (OpenAPI document and generated client type); `RULES` 2; the log line carries host, path and status
- [x] 5 Tests: homepage move, `/feed` to `/feed/`, a fourth hop, another site, a hop robots disallows, final address in the config, hops in the budget, store reads
- [x] 6 Spec delta (boards), module doc comment, changelog entry file
- [ ] 7 Gates green in CI, terminal recording
