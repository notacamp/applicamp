# Design

## Data (schema `pipeline`)
- `pipeline.preparations`: one row per opportunity — `opportunity_id` (pk), `account_id`,
  `state` (`queued`, `running`, `waiting`, `done`, `stopped`, `failed`), `step` (`read`, `draft`,
  `letter`), `waiting_on` (`site`, `answers`, `cv_approval`, `letter_approval`), `reason` (a
  problem code when stopped or failed), `locale` (for the questions), `done` (the steps done by
  itself: `[{step, at, eur}]`), `updated_at`. It is the guard against running a step twice: a
  step is enqueued only by moving the row to `queued` in the same transaction.
- `brief_questions.proposals`: the ids of the facts an answer proposed, so the CV waits until
  they are kept, dropped or expired.
- `companies.site_source` gains `posting_link`, `posting_email` and `name_guess` beside `board`,
  `apply_host` and `person` (ranked: person > board > apply host > posting > name);
  `companies.site_sought_at` keeps a failed search for 30 days.
- Migration `20261103120000_pipeline__preparations.sql`. No cross-schema reference.

## Events and jobs
- No new domain event: the trigger and the steps are inside `pipeline`. Shortlisting (the stage
  PATCH from Found or Not for me to Shortlisted or later) queues the first step. Whenever the
  person acts on the job (opening its brief, typing the site, answering, skipping, approving the
  CV or the letter), `nudge` checks what a waiting preparation waits on, from stored rows only
  and the `facts_waiting` port, and queues the next step when the wait is over; a job never
  prepared is started the same way. Each queue enqueues one kernel job
  `pipeline.prepare { opportunity }`, `max_attempts = 1` (a failure is shown, never retried by
  itself); a job that finds nothing queued does nothing.
- Before each step the job re-reads the stage and the goal; it stops with `not_shortlisted` or
  `find_only`. Stop (or the stage moving back) while a step runs wins over its outcome.

## Ports
- `Neighbours::autonomy(account) -> Option<Autonomy>` (profile's goal; `None` = not answered,
  treated as Prepare). Implemented in `applicamp-server` from `profile::matching::autonomy`.
- `Neighbours::home_page(site) -> Result<String, SiteFailure>`: robots.txt and the home page
  only (`sources::pages::home_text`: same reader, 300 KB, same host, a redirect to another host is
  unreachable). The pipeline decides whether the text names the company (`site::names`). No model
  call; at most 3 per company, and a company not found is not looked for again for 30 days.
- `Neighbours::facts_waiting(account, ids)`: how many proposed facts still wait for Keep or Drop
  (`profile::features::grill::waiting_among`).

## Finding the website (`brief/site.rs`, no model)
1. Person → board → apply host, as today.
2. The posting's text: `https?://` links and `@domain` addresses; drop job boards, tracking
   systems, the boards the posting was seen on, webmail and link shorteners. Keep a domain whose
   registrable name contains the company's core name (its name without Inc, Ltd, GmbH, Software,
   Technologies, Labs, Group…); else the only one left; else none.
3. Asked, at most 3 home pages: the posting's other domains, then `<core>.com`, `<full>.com`,
   `<core>.io` (letters and digits only, deduplicated); kept when the page names the company in
   words, or as one word for a name of six letters or more. Only the read step (and the Read
   button) asks; opening the brief never does.
4. None: the page asks, as today.
The posting text is untrusted data: a candidate from it is only an address, read under the same
robots and host rules as any other site.

## Scopes and endpoints
- `POST /api/v1/opportunities/{id}/prepare/stop` and `…/prepare/start` (signed-in session, like
  every pipeline endpoint), 404 for another person's opportunity; start answers 409
  `stage_too_early` or `find_only`, 503 `ai_unavailable`. The brief gains `preparation` and
  `prepares_by_itself`, and takes `?locale=` (as does the stage PATCH) for the questions' language.
  Today gains `waiting_on_you`.

## AI
Same calls and roles as today (`complete`: company facts, the comparison, the CV, the letter);
nothing new is asked of the model. Without a model configured nothing runs by itself and the
page shows the buttons as today. Undo: Stop; Remove on any fact; Draft again; a draft nobody
approved still goes after 7 days.

## Caps and budget
Unchanged numbers. A step that would pass a cap or the budget is not started: the row goes to
`stopped` with the cap's problem code, and the page shows the reason and the step's button.
Worst case per job ≈ €0.01 (read 0.002, comparison 0.002, CV, letter 0.002).
