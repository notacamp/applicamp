# Design

## Rules (code, `crates/modules/sources/src/requirements.rs`)
Given goals and the job's eligibility facts:
| Goal | Passes when |
|---|---|
| `work_mode` = only `remote` | `remote = yes` **and** `open_to_candidate = yes` |
| `work_mode` includes `hybrid`/`onsite` | `remote = yes and open_to_candidate = yes`, or the job's place is one of `locations` (or within `relocate`'s reach) |
| `relocate` = `no` | the job can be done from the person's location (`open_to_candidate = yes`) or is in one of `locations` |
| `languages_for_work` set (else the profile's languages at professional level or better) | every `working_languages` entry is among them (English assumed only if listed) |
| `cash_floor` set | stated salary max (converted to the floor's period and currency with a fixed table; unknown currency → pass with a gap) ≥ floor; no salary stated → pass with the gap "Salary not stated" |
| `avoid` | handled by the model's score as today (soft) |
A job failing any rule gets `excluded_reason` (`not_remote`, `not_open_to_location`, `language`, `below_floor`) and is not added. The person's location = `goals.locations[0]`, else `profile.basics.location`.

## Model output (per job, strict JSON)
`{ ref, remote: "yes"|"no"|"unclear", open_to_candidate: "yes"|"no"|"unclear", region_note: string (≤ 80 chars, e.g. "Germany or Austria only"), working_languages: [ISO 639-1], salary: {min?, max?, currency?, period?} | null, match: 0-100, why: [≤3], gaps: [≤2] }`. The prompt states the person's location and requirements, says posting text is untrusted data, and asks to answer the facts only from the posting (unclear when not stated). `match` is the fit **among** eligible jobs; it no longer carries the requirements.

## Pre-filter (no model)
Structured fields only: Ashby `isRemote=false` or `workplaceType ∈ {OnSite, Hybrid}`, Lever `workplaceType ∈ {onsite, hybrid}` → drop when goals are remote-only. Greenhouse/Arbeitnow/HN have no reliable field (Arbeitnow's `remote` flag is ignored): they go to the model. Pre-rank prefers jobs whose structured location field names Remote, Europe/EMEA/EU, Portugal or a `locations` city.

## Existing opportunities
A one-off job at deploy (`sources.recheck_requirements`, idempotent) re-asks the facts for `found`/`shortlisted` opportunities without them (≤ 30 jobs, within the spend limits) and sets `hidden_reason`; nothing else changes. Later stages are never hidden automatically.

## Data
`pipeline.opportunities` gains `remote text`, `open_to_candidate text`, `region_note text`, `working_languages text[]`, `hidden_reason text`. `sources.scores` stores the facts too (cache keyed as before, plus a `rules_version`).

## Client
Pipeline shows hidden ones only under a filter "Hidden (don't meet your requirements)" with the reason per row ("Remote only from Germany or Austria"). The peek shows the facts ("Remote · open to Portugal · English"). Search line: "… 9 matched, 14 dropped for your requirements".

## Testing
Rules table tests (each row, unclear handling, language fallback, salary conversion); scoring with a fake model returning each fact combination; the three prod cases as fixtures (Germany/Austria-only remote → dropped `not_open_to_location`; German-only posting → `language`; remote unstated → `not_remote`); pre-filter on Ashby/Lever fields; recheck job hides and is idempotent.
