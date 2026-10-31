# Tasks
- [x] 1 registry table + seed file (approved / candidate / declined with reasons)
- [x] 2 adapters json_api + rss (+ jsonld_sitemap stub), fixtures per seeded board, rate limits/back-off/cache
- [x] 3 search: approved boards only, company boards off, regions in the pre-filter, cross-board dedupe + opportunity_sources, expiry
- [x] 4 discovery job (candidates, checks, approval queue), 6-month re-check
- [x] 4b board stats: average salary (EUR/yr, relevant jobs, 30 days), counts and share; recomputed per fetch and per check; country benchmark table (sourced, dated) for country-scoped boards with < 10 salaried jobs
- [x] 5 Boards page (sorted by average salary) (approve/decline/edit/pause/suggest), attribution in the peek
- [x] 5b board_scans table + Scanned tab (per search/discovery run, per board: read → passed → scored by AI → matched, cost, skips/errors) + 7/30-day totals
- [x] 6 OpenAPI + client types, changelog entry, gates green, recording 375 + desktop (added by the lead)
