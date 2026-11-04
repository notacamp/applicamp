# Tasks
- [x] 1 The positions' prompt names `location`, `summary` (never a place), `highlights` and the dates line
- [x] 2 `imports/repair.rs`: a place summary moves to `location`, a title said again goes, bullets become highlights, dates become `YYYY-MM` (no model); unit tests
- [x] 3 An empty section whose heading is in the text is asked again, once, on the text under its heading; unit tests for the slices
- [x] 4 Merge repairs a matched position (place summary, empty end date, location and summary, highlights once each); unit and HTTP tests
- [x] 5 A LinkedIn-shaped PDF fixture and an HTTP test: no place in a summary, education and languages read on the second try
- [x] 6 Review: "Found …" per section, plain copy in four languages; a PDF picked as the export is read as a PDF; vitest
- [x] 7 Fake model answers the import wrong in the known ways; filmed e2e (desktop and 375px)
- [x] 8 Changelog entry file; OpenSpec change archived; OpenAPI and client types unchanged (no API change)
- [x] 9 Gates green in CI; recording at 375px and desktop in the PR
