# Tasks
- [x] 1 The positions' prompt names `location`, `summary` (never a place), `highlights`, the dates line and when `endDate` is left out; invented examples
- [x] 2 `imports/repair.rs`: a place summary (or a place on its first line) moves to `location`, a title said again goes, bullets become highlights, dates become `YYYY-MM` (no model); a cautious place test; unit tests for every case in the review
- [x] 3 An empty section whose heading is in the text is asked again, once, on the text under its heading (the headline and the About text from the top of the document), at most 8,000 characters; a failure keeps the draft; a limit keeps it and says so (`stopped`, a migration)
- [x] 4 Merge matches positions one to one (company and start, then title) and touches a summary only with a real paragraph; unit and HTTP tests
- [x] 5 A LinkedIn-shaped PDF fixture and HTTP tests: no place in a summary, education and languages read on the second try, a failed or refused second try
- [x] 6 Review: "Found …" per section, "No achievements found", the limit note, plain copy in four languages; a PDF picked as the export is read as a PDF; vitest
- [x] 7 Fake model answers the import wrong in the known ways; filmed e2e (desktop and 375px)
- [x] 8 Changelog entry file; OpenSpec change archived; OpenAPI and client types regenerated
- [x] 9 Gates green in CI; recording at 375px and desktop in the PR
