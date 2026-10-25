# 0007 — Tailored CVs: JSON Resume data, fact-check gate, Typst PDFs

**Status:** Accepted · 2026-10-25

## Context
A CV customised for each application is a core feature, not an add-on.

## Decision
- The master CV is data in a JSON Resume–compatible schema, built from the
  profile import and edited in the app.
- A tailored variant per opportunity reorders, trims and rewrites; every claim
  must trace to the master CV or profile (**fact-check gate**), or it is
  blocked and shown.
- PDFs render with **Typst** as a Rust library: no browser, embedded fonts,
  deterministic output, four languages.
- After rendering, the PDF's text is extracted and checked for every section
  and the job's keywords, so ATS parsers read what we wrote.
- Each PDF is versioned on its opportunity and stored privately.

## Alternatives
Headless Chromium print-to-PDF (heavier, less deterministic); Reactive Resume
as a service (another app to run). Its schema and template ideas are reused (MIT).
