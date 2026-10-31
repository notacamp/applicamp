# The tailored CV as a PDF you can see, checked by reading it back

Issue: https://github.com/notacamp/applicamp/issues/38 (part of #11, tailored CVs; part B, after #37)

## Why
The owner's words: "i want to be able to see those cv pdfs". Part A approves a
CV's text. This change makes it a real PDF, shows it in the app, and makes sure
what a recruiter's system reads out of it is what the person approved: a PDF
can lose a heading (letter-spaced type reads "EX PERIENCE"), a line or a link.
So the text is read back out of the PDF and compared, and a PDF that fails is
never kept. Prototype approved: tabs "5 · PDF & ATS check" and "6 · Your CVs".

## What changes
- **Approve renders the PDF**, inside the server (Typst as a library: no
  browser, no CLI, no network, fonts embedded), with Clean or Compact on 1 or 2
  pages (Clean, 1 page by default). Headings are the standard ones in the CV's
  language. The person's text is passed to a fixed template as data, never
  interpolated into markup.
- **Read-back, no AI**: every heading, every line, the job keywords the review
  showed as on the CV, name, email and links must be found in the PDF's text,
  and the page count must fit. Otherwise nothing is approved and the reasons
  are shown. A CV that does not fit says "Doesn't fit on 1 page: switch to 2
  pages or remove a few lines".
- **Versions are kept** (`pipeline.cv_versions`, the PDF stored with the
  account, never in a public bucket) with the read-back results. A new
  template or length on an approved CV is a new version of the same approved
  text with no AI call; old versions are never overwritten. A new language is a
  new draft.
- **See it**: `GET /api/v1/opportunities/{id}/cv/versions/{n}/pdf` (owner only,
  inline, or `?download=1`), `GET /api/v1/cvs` (every version and live draft),
  a PDF viewer (PDF.js, loaded when first opened, from our own bundle), a
  Versions section and the PDF check on the review page, the peek's View PDF,
  and a CVs page in the sidebar (All / Approved / Drafts).

## Cost
No AI calls at all. CPU: one Typst compile and one text extraction per render,
on a blocking thread, at most two at once, 10 s timeout, about a second.
Storage: 30-50 KB per version in Postgres, at most 20 versions per
opportunity, deleted with the opportunity. The server binary grows by the
Typst compiler and its embedded Libertinus Serif fonts.
