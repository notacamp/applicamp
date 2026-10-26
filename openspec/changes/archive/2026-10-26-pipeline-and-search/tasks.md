# Tasks
## 1. Server
- [x] 1.1 `pipeline` module: schema, opportunities API, events
- [x] 1.2 `sources` module: boards seed list, readers (Greenhouse, Lever, Ashby, Arbeitnow, HN), cache, rate limits
- [x] 1.3 Search job: hard limits, pre-rank, scoring via `complete` (batches), keep ≥ 65 (≤ 25), progress, caps; nightly schedule
- [x] 1.4 Spend limits in crates/ai (cost per call from OpenRouter, pre-call budget check, account daily/monthly + instance monthly caps, `applicamp ai usage`), score cache, estimate endpoint
- [x] 1.5 Add by link (ATS hosts only), `GET /api/v1/today`
## 2. Client
- [x] 2.1 Pipeline: table + board views, filters, side peek, stage change, notes, Not for me, New (link), Search now with progress
- [x] 2.2 Today: state-aware content and next step
## 3. Finish
- [x] 3.1 Tests (fixtures per source, injection case, dedupe, SSRF refusal), e2e desktop + phone, OpenAPI + client types, changelog entry, gates green, recording 375 + desktop
