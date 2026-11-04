# Design

## Data (schema `pipeline`)
- `pipeline.cv_drafts` gains the draft's preview: `preview_pdf bytea`, `preview_template`,
  `preview_length`, `preview_pages`, `preview_checks jsonb` (the read-back), `preview_of` (a SHA-256
  of the text it was made from), `preview_error` (a short
  code: `cv_doesnt_fit`, `cv_pdf_unavailable`), `preview_at`. All nullable; cleared when the draft is
  approved (its version holds the PDF) or put away. Migration
  `20261104145120_pipeline__cv_preview.sql`. No cross-schema reference.
- A line of a draft's `content` may carry `fixed_by_app`, `original` (what it said before) and `ask`
  (`question`, `proposal`). Older drafts read with the defaults.
- The profile keeps a "yes" as a fact with `status: confirmed_assumption` and `basis: "The CV draft
  for <company>"`, through the new port `Neighbours::keep_assumption` (implemented in the server with
  `profile::matching::keep_assumption`, the same checks as `keep_fact`: the position must still be
  the one the CV was made from).

## The preview
- `preview::render_draft` is the one way a draft becomes a PDF (Approve, a new version, the preview):
  the same Typst world, two render slots, a 10 s compile timeout, and now a 15 s wait for a slot
  (`busy`). A strict render refuses a PDF whose read-back fails (Approve); the preview keeps the
  read-back as a note.
- After `create`, the preparation's draft step, `change_line`, `confirm_line` and `put_back`, the
  preview is rendered in the request once the draft is saved, starting from the layout the person
  chose (else Clean, 1 page), then the same template on 2 pages, then Compact on 2. A failure never
  fails the change: it is stored as a short code and logged with `tracing::warn!` (document, cause
  kind, the compiler's error/warning/hint counts), never the CV's text.
- `POST /api/v1/opportunities/{id}/cv/drafts/{n}/preview` `{template, length}` renders that layout only
  (409 `cv_doesnt_fit`, the preview kept as it was). `GET …/drafts/{n}/preview.pdf` serves it inline,
  `?download=1` as `<Name> CV - <Company> (draft).pdf`, `private, no-store`, `nosniff`; another
  account's draft, or a draft with no preview, is 404.
- Approve keeps the preview's bytes when they were made from the draft's text as it is now (the
  preview keeps a SHA-256 of that text, `preview_of`), in the layout asked for (the preview's when
  left out), and its read-back passed; else it renders.

## The check (`kernel::claims::classified`, `pipeline::cv::check`)
Each claim has a kind: number, tool, acronym, name. A claim the cited sources do not say is flagged
unless it is a name that is a plain word (Senior, Developer, Stack, a month), a place or language the
profile names, or, in the summary, a word or acronym of the posting; the headline may also say any
position's title. A number, a tool (the technology list and the profile's skills) and an employer's
name (the profile's companies, the job's) are always claims. A bullet citing another position's source
is flagged (`other_position`), and that source does not back it.

## The fix round (`cv::fix`, `cv::fix_prompt`)
One `complete` call per draft, only when a line is flagged (never a skill), at temperature 0 with
3,000 tokens of answer, through `brief::ask_attempt` (the account's budget reserves first). The model
is shown each flagged line, why, and the sources it may use (not a company fact outside the summary,
not another position's): data, not instructions, never the person's name or contact. It answers per
line a rewrite, or a question with a first-person fact. A rewrite is checked like any line; one that
fails is dropped and the line keeps its question. A fact is kept as the proposal only when it backs
the line; otherwise "Yes, keep it" keeps the line's own words. One round; a call that fails leaves the
flags to the person. Role `complete`; without a model no draft is made at all, as today.

## Scopes and endpoints
Signed-in session, like every pipeline endpoint; another person's opportunity is 404.
`POST …/lines/{line_id}/confirm` takes `{proposal: true}` (409 `cannot_keep` for a company fact used
as an achievement, `not_flagged` for a line that passes). `GET …/cv` adds to the draft `preview`,
`fixed_by_app`, `waiting` (`line_id`, `kind`, `text`, `question`, `proposal`, `role`, `flags`) and
`needs_version`.

## Undo
`POST …/drafts/{n}/unapprove` puts a draft approved less than 15 s ago (`cv::UNDO_WINDOW`) back as a
draft: its version row goes (the only exception to "versions are never overwritten", and only in that
moment), the preview is made again, and a letter step the preparation queued goes back to waiting
on the CV. The letter step's job is enqueued 15 s late for that reason; once it runs, or a letter
draft exists, Undo is `too_late`. `GET …/cv` carries `undo_until` on the draft and `next` (`letter`,
its cost, when it starts). Change it, Remove it and put back stay; a kept fact is removed from the profile as any fact is; Make
again makes a new draft; a draft nobody approved still goes after 7 days.

## Cost
At most one more small call per draft (about €0.0005), within the same caps (10 drafts a job, 20 a
day) and the account's and instance's spending limits. Rendering a preview makes no model call.
