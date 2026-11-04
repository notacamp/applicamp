# Prepare by itself: a shortlisted job gets ready, waiting only on you

Issue: https://github.com/notacamp/applicamp/issues/70 (first follow-up of #68, ADR 0010)

Module: `pipeline` (reads the Autonomy goal from `profile` through its port; `server` composes it).

## Why
Preparing one shortlisted job takes up to four clicks (read the company, answer, draft the CV,
write the letter), and when the board and apply link give no website the person types it, even
when the posting names it (`jobs@perrystreet.com`). The Autonomy goal ("Prepare applications")
is collected and never read. ADR 0010: the app does the work first; approvals and control stay.

## What changes
- **The website is found, then asked.** After the person's, the board's and the apply link's:
  a link or email address in the posting whose domain is the company's name, then at most three
  home pages asked (the posting's other domains, then domains made from the company's name), each
  kept only when its robots.txt allows its home page and that page names the company. No model call. The page says where the site came from, and
  the person can change it (their address always wins and re-reads). Asked only when none fits.
- **Shortlisting starts a chain** when the Autonomy goal is "Prepare" (or not answered):
  read the company and compare the job → stop while a question is open → draft the CV. Approving
  the CV drafts the cover letter and answers. Opening Prepare on a Shortlisted job that was never
  prepared starts the same chain, so jobs shortlisted before this change are covered.
- **"Draft the CV" reads first.** A draft for a job whose company was never read reads it in the
  same step when it can; when it cannot (no website, robots, cap), the draft says so.
- **What the app does without the person:** find the website, read the company, compare the job,
  draft the CV, draft the letter and answers. **What still waits on them:** answering the open
  questions, keeping facts, approving the CV, approving the letter, every stage move.
- **Control:** each step done by itself says so with its cost; Stop cancels what is still to
  run for that job (so does moving it out of Shortlisted or later, or choosing "Find only");
  Read again, Draft again and Try again stay. A failed step is never retried by itself.
- Today lists jobs waiting on the person's answers or approval.

## Non-goals
- Proposing which jobs to shortlist, extracting a pasted link's role and company, suggested
  answers to questions, applying, recruiter mail (later items on #68).
- Running on Found jobs, on a timer, or for an open-web search. No search engine is used.
- Past employers' websites in Tell me about you (same finder; its own change).
