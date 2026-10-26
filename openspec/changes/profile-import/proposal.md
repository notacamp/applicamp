# Profile import: your work history in, from LinkedIn or a CV

Issue: https://github.com/notacamp/applicamp/issues/4

## Why
Agents can only match jobs and write CVs from what the app knows about you.
LinkedIn sign-in gives a name and an address, nothing more (ADR 0006), so the
profile comes from files you already have.

## What changes
- A `profile` module owning the **master profile**: headline, summary,
  positions, education, skills, languages, certifications, projects, links
  (JSON Resume–compatible shape, ADR 0007).
- Three ways in, each producing a **draft you review before it is saved**:
  1. The **LinkedIn data export** ZIP: read deterministically from its CSVs.
  2. The **LinkedIn profile PDF** or **your CV as a PDF**: text extracted on the
     server, then structured by the `complete` model.
  3. **Pasted text** (a CV or a profile): structured by the `complete` model.
- A model-structured draft keeps only values that appear in the source text
  (a **source check**); anything else is dropped and counted, never invented.
- The Profile & goals page shows the profile Notion-style (properties, lists),
  editable by hand; Today's "Set up your profile" opens the import.
- The AI gateway crate (`applicamp-ai`, trimmed from Campbooks Next's) with the
  `complete` role over OpenRouter.

## Modules touched
`profile` (new), `ai` (new crate). Client: `modules/profile`.

## Non-goals
- The goals interview (its own change, `goals-interview`).
- Word/DOCX files, images, OCR of scanned PDFs.
- LinkedIn's member-data API (needs LinkedIn approval; later).
- Keeping the uploaded files: they are read and discarded.
