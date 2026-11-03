# Design

## Data (schema `pipeline`)
- `pipeline.preparations`: one row per opportunity — `opportunity_id` (pk), `account_id`,
  `state` (`queued`, `running`, `waiting_answers`, `waiting_approval`, `done`, `stopped`,
  `failed`), `step` (`read`, `draft`, `letter`), `reason` (a problem code when stopped by a cap,
  the budget or a failure), `spent_eur`, `updated_at`. It is the guard against running a step
  twice: a step is enqueued only by moving the row to `queued` in the same transaction.
- `companies.site_source` gains `posting_link`, `posting_email` and `name_guess` beside `board`,
  `apply_host` and `person`.
- Migration `<ts>_pipeline__preparations.sql`. No cross-schema reference.

## Events and jobs
- No new domain event: the trigger and the steps are inside `pipeline`. Shortlisting (the stage
  PATCH from Found to Shortlisted or later), opening the brief, keeping the last answer's facts
  and approving a CV each enqueue one kernel job `pipeline.prepare { opportunity, step }` in
  their own transaction, `max_attempts = 1` (a failure is shown, never retried by itself).
- Before each step the job re-reads the row, the stage and the goal; it does nothing when the
  row is `stopped`, the stage went back to Found or Not for me, or the goal is "Find only".

## Ports
- `Neighbours::autonomy(account) -> Option<Autonomy>` (profile's goal; `None` = not answered,
  treated as Prepare). Implemented in `applicamp-server`.
- `Neighbours::names_company(site, name) -> Result<bool, SiteFailure>`: fetches robots.txt and
  the home page only (same 300 KB / 10 s limits, same host, no redirect to another host) and
  says whether the page's title or text names the company. Fetches here count toward
  nothing billed and make no model call; at most 3 per opportunity, once per company (the
  outcome is kept on the company so it is not guessed again for 30 days).

## Finding the website (`brief/site.rs`, no model)
1. Person → board → apply host, as today.
2. The posting's text: `https?://` links and `@domain` addresses; drop job boards, tracking
   systems, the boards the posting was seen on, webmail and link shorteners. Keep a domain whose
   registrable name contains the company's core name (its name without Inc, Ltd, GmbH, Software,
   Technologies, Labs, Group…); else the only one left; else none.
3. Name guesses: `<core>.com`, `<full>.com`, `<core>.io` (letters and digits only, deduplicated),
   tried in order until one `names_company`.
4. None: the page asks, as today.
The posting text is untrusted data: a candidate from it is only an address, read under the same
robots and host rules as any other site.

## Scopes and endpoints
- `POST /api/v1/opportunities/{id}/prepare/stop` and `…/prepare/start` (scope `pipeline:write`),
  404 for another person's opportunity. The brief's GET gains `preparation`
  (`state`, `step`, `reason`, `spent_eur`) and the site's `source`.

## AI
Same calls and roles as today (`complete`: company facts, the comparison, the CV, the letter);
nothing new is asked of the model. Without a model configured nothing runs by itself and the
page shows the buttons as today. Undo: Stop; Remove on any fact; Draft again; a draft nobody
approved still goes after 7 days.

## Caps and budget
Unchanged numbers. A step that would pass a cap or the budget is not started: the row goes to
`stopped` with the cap's problem code, and the page shows the reason and the step's button.
Worst case per job ≈ €0.01 (read 0.002, comparison 0.002, CV, letter 0.002).
