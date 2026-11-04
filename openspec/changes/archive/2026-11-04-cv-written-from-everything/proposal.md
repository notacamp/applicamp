# The CV is written from everything the profile knows, on a stronger model

Issue: https://github.com/notacamp/applicamp/issues/73 (second of two changes; ADR 0010)

Modules: `pipeline` (the draft, the review, shortlist proposals), `applicamp-ai` (the `write` role),
`profile` (the languages the person works in, through the pipeline's port), `server` (composition).

## Why
Drafts come out thin: positions are dropped when their only sourced text is short, the person's
background, projects and certificates never reach the model, the draft model is the cheap triage
model, the posting's language picks the CV's (a German posting gives a German CV), and positions are
printed in the model's order. Jobs that fit well wait in Found until the person shortlists them by
hand, even when their Autonomy goal says "Prepare".

## What changes
- **A `write` role** in `applicamp-ai` for longer writing (CVs, letters): its own model, priced and
  capped like `complete`, recorded in the same ledger. Without it, `complete` writes (one model still
  runs everything). The CV draft goes to it.
- **The draft uses everything**: background, projects and certificates join the positions, kept facts,
  education, languages, what the job wants and the company facts. Every position with sourced text
  is shown unless it is both of no help and older than ten years; 3 to 6 bullets for the two most
  recent, 1 to 3 for older ones; newest first whatever order the model gives; the positions left out
  are named in one "Earlier" line by the app (title, company, years; no model text).
- **The person's language**: the CV is written in the first language of their goals (else one their
  profile says they speak well, else English) unless they ask for another; the posting's language is
  only offered.
- **What the job wants, a second time**: when the job wants something the profile shows and the
  draft does not say, one more call asks for up to three bullets from the person's own sources,
  checked and fixed like any line. The review shows each want as on the CV or why not.
- **Shortlist proposals**: a found job with a match of 75 or more, open to the person, is proposed
  (`GET /api/v1/opportunities?proposed=true`). After a search, with the Autonomy goal "Prepare" (or
  not answered), the app shortlists the best three a day by itself and starts their preparation;
  never past the daily CV cap or the spending limits.
- **The review page** per the approved prototype: the PDF first; beside it what needs the person,
  what the job wants, what the app did and what it cost, layout and language as labelled choices,
  Approve, Write it again and Download.

## What the app does by itself, and what still waits on the person
- **By itself:** write the CV from everything the profile knows, on the stronger model; add what the
  job wants that the profile shows; pick the person's language; name the older positions; propose
  jobs that fit; with "Prepare", shortlist the best three a day and prepare them.
- **Still waits on the person:** answering questions, approving the CV, every approval after it,
  and every stage move other than the app's own few shortlists (which they can move back or set
  aside). With "Find only" nothing is shortlisted for them.

## Non-goals
- Today's cards for proposals (Keep / Drop): #79 builds Today; this change gives it the data.
- The cover letter's writing model and prompt (unchanged, still `complete`).
- Prices for models the table does not know: the instance sets them.
