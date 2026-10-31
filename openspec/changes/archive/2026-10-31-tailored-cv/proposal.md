# A tailored CV per opportunity: draft, fact check, review and approve

Issue: https://github.com/notacamp/applicamp/issues/37 (part of #11, tailored CVs; the third change, after Grill me #33 and company facts #34; the PDF, its viewer and the versions list are part B, #38)

## Why
The owner's words: a dedicated CV per opportunity, "based on real facts about
the company and my job". #33 gave the profile facts in the person's own words
and #34 gave the company's facts and what the job wants. This change drafts
the CV from them, and makes sure it cannot say anything the profile does not.
A model asked to write a CV adds numbers, tools and employers that sound right.
Here every line cites the pieces of the profile it comes from, a check that
uses no model compares the two, and a line it cannot trace is blocked until
the person fixes it, says it is true in their own words, or removes it.
Nothing can be approved while a line is blocked. Prototype approved 2026-10-30
(tab "4 · Review the CV"): https://claude.ai/artifact/KmztFvd4e7P3uiWSvbUSX5

## What changes
- **Draft the CV**, from Prepare (Shortlisted or later): one model call turns
  the posting, what the job wants, the company facts the person has not
  removed and the profile into a headline, a summary, positions with bullets
  and a skills list. Every line cites the profile sources it comes from
  (a position's summary, a highlight, a kept fact, the summary, the skills,
  education, languages; a company fact, in the summary only). The model is
  never shown a name, an email, an address or a link: the server prints the
  contact itself. The draft is written in the posting's language, else English.
- **The fact check, no model.** Each line may say only what the sources it
  cites say: every claim of the line (a number, a tool, an acronym, a name; the
  claim rules of the draft source check) must be in their text. A source id
  that does not exist counts as no source. A company fact cited outside the
  summary is flagged ("company facts can't be your achievements"). Every skill
  must be one on the profile (aliases such as Postgres and PostgreSQL count).
  A line that fails is blocked and says why, naming the claim.
- **Review** at `/pipeline/{id}/cv`: the CV as a page, each line marked
  Reworded (opening onto "From your profile" with the cited words and the
  claims picked out), Moved up, As in profile or Edited; a blocked line
  carries its reason and: **Edit** (checked again against the same sources),
  **It's true: add it to my profile** (the person's own words become a kept
  fact on the line's position and the line is checked again with it as a
  source; for a skill, the skill joins the profile) and **Remove**. What the
  draft left out is listed ("Left out for this job") with **Put back**, which
  restores a position or a line word for word, with its own source.
- **Approve** stays off while any line is blocked. Approving freezes the draft
  as the next version (v1, v2…) of the opportunity; part B renders it.
- **Beside the CV**: whether every line traces to the profile (with "Go to the
  first one"), the job's keywords on the CV, in the profile only, or not in it
  (no model; keywords the person lacks are never added for them), what changed
  from the profile, and the spend.
- The peek's Tailored CV block says where the CV is: no draft, a draft with
  n lines that need the person, or vN approved.

## Modules touched
`pipeline` (server: new slice `features/cv`, schema `pipeline`, table
`cv_drafts`; client: the review route and page, the draft slot on Prepare, the
peek block), `profile` (server: add a fact in the person's own words to a
position, add a skill, the whole profile as a CV is made from it),
`applicamp-kernel` (the claim rules and `squash` move here from the profile
module so both modules use them; skill aliases in `tech`), `applicamp-server`
(wiring), and the fake model in `clients/app/scripts/fake-jobs.ts`.

## Cost
AI role `complete`, temperature 0, no background calls: every call starts with
a click.

| Step | Calls | Estimate |
|---|---|---|
| Draft the CV (and Make again) | 1 | about 4k tokens in, 1k out, about EUR 0.002 |

The fact check, edits, "It's true", Put back, Approve, Discard and every read
of the page make no model call. The call goes through the existing budget,
which reserves its worst case first: EUR 0.30 a day and EUR 5 a month per
account, EUR 15 a month for the instance. Over a limit the call is refused
before it is sent, the page says so in plain words, and nothing is retried.

New hard caps on top, as problems (HTTP 429) refused before any call:
**10 drafts per opportunity** and **20 drafts per account per UTC day**.

## Non-goals
- The PDF, its viewer, templates, layout choices, the versions page and
  downloads (part B, #38). An approved version is only text, for now.
- A cover letter or form answers.
- Any call in the background, or a draft made without a click.
- Letting the model add keywords the person lacks, or a "use what your profile
  says" replacement for a blocked line (the person can edit, say it is true,
  or remove it).
- Translating an existing draft: another language is a new draft.
