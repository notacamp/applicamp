# Every CV draft is a PDF you can see, and the app clears its own flags

Issue: https://github.com/notacamp/applicamp/issues/73 (first of two changes; ADR 0010)

Modules: `pipeline` (the tailored CV), `profile` (a fact the person said yes to, kept as a confirmed
assumption, through the pipeline's port), `kernel` (the claim check names the kind of each claim).

## Why
No PDF of a CV has ever been made on the hosted instance. A PDF is made only when the person approves,
and Approve is off while any line is flagged. The check flags any word with a capital in the middle of
a sentence, so a headline like "Senior Full Stack Developer" or a summary that says "Lisbon" or
"Europe" is flagged, while the prompt asks the model to use the job's own words. The person is then
asked to clear each flag by typing in a box. The owner (2026-11-04): "the CV page doesn't generate PDF
CVs … the CV builder is asking me to do too much work … the app and AI should do most of the work and
I'm here to review it and approve."

## What changes
- **A PDF for every draft, at once.** When a draft is made, by a click or by the preparation of a job,
  and after every change to it, the server renders it as a PDF: Clean on one page, else Clean on two,
  else Compact on two. The review page shows it as its main content. The person can pick another
  layout (no AI) and download the draft. Approving keeps the same PDF when nothing changed. A PDF that
  could not be made says so and its cause is logged (kinds and counts, never the CV's text).
- **The check flags only claims that matter.** Numbers, tools, employers, acronyms, dates, and places
  the profile does not name. Words such as Senior, Developer or Stack are not claims; a place the
  profile names is allowed anywhere; the summary may use the posting's words (not its numbers, tools
  or employers). A bullet may cite only its own position's lines.
- **The app clears flags before asking.** After the check, one small call rewrites each flagged line
  to say only what its sources say. A rewrite that passes the check replaces the line (marked as fixed
  by the app). For the rest, the app asks one plain question with the fact it would keep: "Is this
  true?" with Yes, keep it / Change it / Remove it.
- An approved draft without a PDF (approved before PDFs existed) offers to make it.

## What the app does by itself, and what still waits on the person
- **By itself:** render the PDF of every draft and after every change; pick a layout that fits;
  rewrite flagged lines to what the profile says (one call, within the same caps and budget); propose
  the fact a "yes" would keep.
- **Still waits on the person:** answering each question (Yes, keep it / Change it / Remove it);
  nothing reaches the profile until they say yes; approving the CV. The fact-check gate before
  approval stays (AGENTS.md). Nothing is sent to anyone.

## Non-goals
- A stronger model, a richer draft (projects, certificates, background), the CV's language from the
  person's goals, the "What the job wants" panel: the next change (`cv-written-from-everything`).
- Proposing jobs to shortlist; Today's cards (#79).
- Changing the cover letter's check.
