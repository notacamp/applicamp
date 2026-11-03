# Watch the companies whose jobs keep fitting you, checked daily on their own job board

Issue: https://github.com/notacamp/applicamp/issues/20 (builds on #5, the job-board search and the Greenhouse, Lever and Ashby readers it keeps; on #19, the hard requirements; and on #34, the company facts)
Modules: sources (`features/watch`, the search, the scans), pipeline (Today, the origin of an opportunity), and the client's sources and pipeline modules, plus the Companies page.

## Why
The owner asked (2026-10-27) for the app to "find companies that are perfect matches, ask if you want to watch their job
boards, and every day look and notify you if something that matches your profile comes up". A search reads job boards,
which list what is posted; a company's own job board lists everything it has open, first. The readers for Greenhouse,
Lever and Ashby already exist and were kept for this. The prototype was approved by the owner ("looks good") with its three
decisions: suggestions come from the person's own matches and not from an AI search of the web; the daily check runs with the
nightly search, at 21:00 Lisbon time, in one batch and one cost line; only public job boards and JobPosting data are read,
never LinkedIn or Indeed, with robots.txt respected.

## What changes
- **Suggestions, no model.** A company is suggested when, in the last 8 weeks, the person has at least 2 matches from it, or
  1 they shortlisted. It must have a website on record, and is never LinkedIn, Indeed, Glassdoor or a hiring platform. Each
  reason is worked out from the postings' own fields and cites the postings (or the company facts already read for a CV) it
  comes from: "N of their jobs fit you in the last W weeks", the person's skills the postings name, remote in N of M postings,
  the places and the pay the postings state. They are worked out when a search ends and when the Companies page is opened and
  they are over an hour old. "Not interested" hides a company for good; Watch moves it to Watching.
- **Finding the job board, no model.** On Watch, a background job (`sources.find_board`) uses the board a posting of the company
  already came from (a Greenhouse, Lever or Ashby link), else reads the company's own site where robots.txt allows (home page,
  the careers pages it links, `/careers`, `/jobs`, `/join-us`, `/work-with-us`) for a Greenhouse, Lever or Ashby link, embed or
  token, or schema.org `JobPosting` data. The board is read once to confirm it lists jobs, and its postings are noted as seen,
  so the watch is "from today". With nothing found, the watch says "Couldn't find their job board" and offers a box for the
  careers page, which is looked at the same way. "Watch a company yourself" pastes a careers page (or a link to a board) and runs
  the same detection; the company is made with origin `pasted`.
- **The daily check, inside the search.** A search (the nightly one, and "Search now") also reads each watched company's job
  board when it was last read more than 20 hours ago (`APPLICAMP_SOURCES__WATCH_EVERY_SECS`), compares it with the postings
  already seen, and sends only the new postings that pass the person's hard limits to scoring, in the same batches, within the same
  spending limits. A match (65 or more, the search's own threshold) that meets the requirements lands in the Pipeline as Found,
  credited to the company's own job board and deduplicated against what the pipeline already holds. The nightly job also starts
  a search for a person whose watched board is due, even when no job board has anything new. What a search did on each watched
  board is a scan row like a job board's. A board that fails keeps its watch and says why; it is not asked again until the next day.
- **Companies page** (`/companies`, in the sidebar): Suggested and Watching tabs with counts, suggestion cards with reasons, Why?,
  Watch and Not interested, the Watching list (board, open jobs, last check or why it failed, Stop watching), "Watch a company
  yourself", and "How it works & cost".
- **Today:** "X posted a job that fits you" for a job found on a watched company's own board in the last 3 days and not yet
  opened (Open it lands in the Pipeline's peek, Not for me sets it aside), and a compact suggestion card.
- **Model kept neutral.** `sources.companies` describes a company and its job board and belongs to no one; `company_watches` holds
  one person's suggestion or watch; `watch_seen` the postings seen. A later "find companies like these" run kind can add rows to
  `companies` with origin `agent` through a typed tool.
- Fakes and tests: three invented company sites per Playwright project (a careers page that links a Greenhouse board, one with
  JobPosting data, one with nothing) whose board lists two more jobs on its second read, one that fits and one open only to Germany
  and Austria; the remote board lists two jobs of one company for a skill; Rust, client and Playwright tests.

## Cost
- Suggesting companies, finding a job board and the daily read of a board: no model, EUR 0.
- New postings that pass the person's hard limits: scored with the nightly search, about EUR 0.0003 each (one batch call for ten).
  Most days a board has nothing new, so no model call at all.
- At most 20 watched companies per account. Everything goes through the AI budget (EUR 0.30 a day, EUR 5 a month); a search stops at
  the limit, and the postings it could not score are not marked as seen, so the next search scores them. Nothing is retried past a
  limit, and a failed board is not read again until the next day.
- Storage: a few rows per company; the seen postings of a board are forgotten 60 days after it stopped listing them.

## Non-goals
- A discovery agent that finds companies on the open web (later, on the agent harness, through the neutral model above).
- Notifying by email or push (the Today card and the Pipeline are the notification for now).
- LinkedIn, Indeed, Glassdoor or any board whose terms forbid reading it; reading a page robots.txt forbids; following a redirect off
  the company's site.
- Reading a company's job pages one by one when it lists them without JobPosting data on its careers page, or any model reading of a page.
- Applying to a job from the Companies page.
