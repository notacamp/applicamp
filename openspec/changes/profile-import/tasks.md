# Tasks
## 1. Server
- [ ] 1.1 `crates/ai`: complete role, retries, usage ledger, daily cap, config
- [ ] 1.2 `profile` module: schema.rs, migrations, events
- [ ] 1.3 LinkedIn ZIP reader with archive guards (fixtures: a synthetic export with invented data)
- [ ] 1.4 PDF text extraction + model structuring + source check (fake complete endpoint in tests)
- [ ] 1.5 Imports endpoints (create/get/apply/discard), profile get/put, expiry job
## 2. Client
- [ ] 2.1 Import panel (ZIP / PDF / paste), upload states and errors
- [ ] 2.2 Review screen (new/changed/same, dropped count, replace/merge/discard)
- [ ] 2.3 Profile view + inline edit; Today CTA wired
## 3. Finish
- [ ] 3.1 OpenAPI + client types regenerated, changelog entry file, gates green in CI, e2e happy path extended (ZIP import), recording at 375px and desktop
