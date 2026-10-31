# Company and job facts for tailored CVs

Issue: https://github.com/notacamp/applicamp/issues/34 (part of #11, tailored CVs; the second of three changes, after Grill me #33)

## Why
The owner's words: the dedicated CV must be "based on real facts about the
company and my job". A model asked to tailor a CV for a company will happily
fill the gaps from memory, and what it remembers about a small company is
often wrong. So before any CV is drafted, Applicamp reads what the company
says about itself on its own website, keeps each fact with the words of the
page that say it, and compares the posting with what the person has done,
asking the few questions where the profile is thin. Prototype approved
2026-10-30 (tab "3 · Company & job"): https://claude.ai/artifact/KmztFvd4e7P3uiWSvbUSX5

## What changes
- **Prepare: company and job**, from the opportunity peek (Shortlisted or
  later): a page at `/pipeline/{id}/prepare`. Nothing is read or asked of the
  model until a click.
- **The company's site.** The job board's company website when it gives one,
  else the apply link's host when that is not a known job board or tracking
  system (a link the person pasted counts as its own apply link), else the
  person types it once. It is kept on the company, so it is asked once.
- **Company facts.** One click reads the posting, the company's home page and
  up to five more pages of the same site linked from it about the company
  (about, company, customers, clients, blog, engineering, team, product,
  careers), where robots.txt allows, and never another host. One model call
  turns the text into short facts, each with the page's exact words and the
  page's link. A fact whose quote is not on the page it cites is left out
  and counted. The person can remove any fact ("Wrong, remove", with Undo); a
  removed fact stays removed on later reads. Cached for 30 days per company,
  shared by every job there; "Read again" reads sooner.
- **What the job wants.** One more call compares the posting with the
  company's facts and a compact profile (roles, dates, kept facts, skill
  names; no name or contact details): up to six things the job wants most,
  each quoting the posting and marked must or nice, and up to three
  questions where the profile is thin, each naming the company fact or the
  posting line it comes from. Cached until the posting changes.
- **Answering.** An answer is read by Grill me's own pipeline and checks
  (the person's words, nothing invented) into facts that wait on the
  position the question is about, or on the most recent one when it names none
  (and the page says so). They are kept, fixed or dropped with Grill me's own
  controls and live on the profile like any other. "I don't have it" on a
  skill question records a skill dismissal (no call); Skip hides the question
  for this job.
- A pasted link to a site that is not a job board has no text yet; the
  posting page read here becomes its description.

## Modules touched
`pipeline` (server: new slice `features/brief`, schema `pipeline`, new
tables for companies, facts, job analysis and questions; client: the peek, a
new route and page), `sources` (server: reading a company's own pages through
the reader's guarded client, `pages`; the board's company website field),
`profile` (server: answering a question about a job through Grill me's
pipeline, a compact profile digest, skill dismissal), `applicamp-server`
(wiring the three), and the fake model and fake company site in
`clients/app/scripts/fake-jobs.ts`.

## Cost
AI role `complete`, temperature 0, no background calls: every call starts with
a click.

| Step | Calls | Estimate |
|---|---|---|
| Company read | 1, cached 30 days per company | about 10k tokens in, 800 out, about EUR 0.002 |
| Job analysis | 1, cached per posting text | about EUR 0.002 |
| Answering a question | 1 small | about EUR 0.0005 |

Fetching pages costs no model call but counts toward the cap below. Removing a
fact, Undo, "I don't have it", Skip, the site and every read of the page make
no call. Every call goes through the existing budget, which reserves its
worst case before it is sent: EUR 0.30 a day and EUR 5 a month per account,
EUR 15 a month for the instance. Over a limit the call is refused before it is
sent, the page says `ai_budget_reached` in plain words, and nothing is
retried until the person asks again.

A new hard cap on top: **20 company reads per account per UTC day** (problem
`company_reads_daily_cap`, HTTP 429). Every read that starts counts, whether
or not the site answered, so a loop of fetches is limited too.

## Non-goals
- Drafting the CV, its fact check and PDF (the third change); the page leaves
  a clear spot for "Draft the CV".
- Any other source of facts: no news, LinkedIn, Glassdoor, review sites or
  search engines, ever.
- Any call or fetch in the background, and re-reading a company on a timer.
- Reading pages behind a login, PDFs or images; a page over 300 KB is cut.
- Changing what Grill me asks, or how Improve my profile works.
