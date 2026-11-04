# Design

## The `write` role (`applicamp-ai`)
`Role::Write` ("longer writing: CVs, letters"), configured like `complete` from
`APPLICAMP_AI__WRITE__{BASE_URL,MODEL,API_KEY(_FILE),INCLUDE_USAGE,REASONING_EFFORT,
REQUIRE_PARAMETERS,PRICE_IN_USD_PER_MTOK,PRICE_OUT_USD_PER_MTOK}`. `Gateway::complete_as(role, …)`
runs any role with the same daily cap, worst-case reservation and ledger (`ai.usage.role` gains
`write`, migration `20261104110000_ai__write_role.sql`); `Gateway::complete` is `complete_as(Complete)`.
`Gateway::writer()` is `write` when configured, else `complete`: the fallback when an instance runs
one model. No price-table entry is guessed: an unknown model is priced high until the instance sets
its price.

## The draft (schema `pipeline`)
- The port's `CvProfile` gains `background`, `projects`, `certificates` and `work_languages` (the
  goals' `languages_for_work`, read by the server from `profile::matching::languages_for_work`).
- The frame adds sources `bg`, `pr{i}` (name: description) and `ce{i}` (name, issuer, date). A bullet
  may cite them (they belong to no position).
- Prompt version 2 on the writer role: up to 14 positions, 12 company facts, 6 wants, 8,000
  characters of posting; `max_tokens` 6,000, then 10,000 when cut off. The judge keeps at most 6
  bullets a position and 30 in all, and sorts positions newest first by their start date; Put back
  keeps that order. Positions left out are printed as "Earlier" lines by the PDF data (no model text;
  the read-back checks them like any line).
- Language: asked for, else the goals' first, else a profile language at a working fluency (native,
  fluent, professional, C1, C2) that is one of the CV's four, else English. `GET …/cv` carries
  `suggested_language` when the posting's differs.
- The wants pass (`cv::wants`): the keyword panel's own reading (no model) finds wants that are in the
  profile but not on the CV; one call on the writer role (3,000 tokens) asks for up to three bullets
  `{position, text, sources}`; they are added, checked, and go through the fix round with the rest.
  None qualifying, no call. `GET …/cv` carries `draft.wants` (`text`, `level`, `covered`, `reason`).
- `cv_drafts.cost_eur` keeps what the draft's calls cost (migration `20261104120000_pipeline__cv_cost.sql`);
  `GET …/cv` carries `done` (the preparation's steps, then the draft with that cost).

## Shortlist proposals (schema `pipeline`)
- `opportunities.proposed_at`, `shortlisted_by` (`person`, `app`), `shortlisted_at` (migration
  `20261104130000_pipeline__shortlist_proposals.sql`). `store::insert` proposes a found job with a match
  of 75 or more whose eligibility is not "not open to the person"; `set_eligibility` withdraws it when
  a requirement rules it out; the stage PATCH and "Not for me" clear it, and a person's shortlist sets
  `shortlisted_by = 'person'`.
- `GET /api/v1/opportunities?proposed=true` (signed-in session) lists them, best match first; every
  opportunity carries `proposed` and `shortlisted_by`.
- Events: the pipeline consumes `sources.search_finished` (its own mirror of the event's payload,
  ids and counts) with `pipeline.after_search`, which queues the job `pipeline.shortlist_proposals`
  (one attempt). It does nothing with "Find only" or without a model, when the daily CV cap is
  reached, or when the day's or month's spending limit is; else it shortlists the best proposals up
  to three per UTC day per account (`shortlisted_by = 'app'`), starts #70's preparation for each and
  records `shortlist` among the steps the preparation did by itself. No model call; the preparation's
  own steps keep their caps and limits.

## States (owner-approved after the design review)
- `companies.read_failure` keeps why the last read failed (`robots_disallows`, `no_pages_readable`,
  `site_unreachable`; cleared by a read; migration `20261104140000_pipeline__pasted_company_text.sql`,
  which also allows pages of kind `pasted`). `GET …/cv` carries `company_read_failure`, `read_eur` and,
  on the draft, `company_facts`.
- `POST /api/v1/opportunities/{id}/brief/paste` `{text}` (1 to 20,000 characters): the text is read
  like a page of the site (`facts_from`, shared with a read): one `complete` call, facts kept only
  when they quote it, the company's facts replaced as a read does, one of the 20 reads a day.
- `GET …/cv` carries `writing` while the preparation's read or draft step is queued or running; the
  page reads the CV again every few seconds until it is there.
- A paid step's button carries no price; the cost is on the line under it (owner decision).

## Scopes and endpoints
Signed-in session as every pipeline endpoint; another person's data is 404. No new endpoint besides
the list filter.

## AI
`write` for the draft and the wants pass (fallback `complete`), `complete` for the fix round, as in
the first change. Undo: Write it again, Change it / Remove it / put back, Stop on a preparation,
moving a job the app shortlisted back to Found or setting it aside.

## Cost
A draft is one larger call on the writer model, at most one small wants call, at most one small fix
call. The instance prices the writer model; the account's and instance's limits bound everything.
