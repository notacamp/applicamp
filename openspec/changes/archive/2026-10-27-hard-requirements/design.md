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

## Implementation notes (where the build differs from, or adds to, the plan above)
- **Where they live unknown.** With neither `goals.locations` nor a profile location,
  `open_to_candidate` cannot be answered, so a remote-only person's jobs need only
  `remote = yes`; the prompt says `<candidate_location>not known</candidate_location>`.
  Likewise "the job's place is one of `locations`" is no requirement when the person named
  no place at all.
- **Their places** are `goals.locations`, else the profile's location. Whether a job's place is
  one of them, and which countries a place names (for `relocate` = within country / within the
  EU), is a small fixed table in `crates/modules/sources/src/places.rs` (country names in
  English, their own and the app's languages, and the bigger cities). A city is not its whole
  country: a job somewhere in "Portugal" is not one in "Lisbon". "Remote", "Anywhere" and
  "Hybrid" never match a place. A place the table does not know names no country: unclear,
  so not met.
- **Working languages.** `goals.languages_for_work` (BCP 47, compared by primary subtag, `nb`/
  `nn` as `no`), else the profile's languages at a professional level or better
  (`crates/modules/profile/src/matching.rs`): LinkedIn's "Native or bilingual", "Full
  professional" and "Professional working"; CEFR B2, C1, C2; the words native, bilingual,
  mother tongue, fluent, proficient, advanced, professional. "Limited working", "Elementary",
  A1–B1, basic, beginner and a plain "Intermediate" are not. A language listed with no fluency
  counts. Profile language names map to ISO 639-1 with a small table (English and the app's
  languages' names for about 30 languages). No languages known: no language requirement. The
  model gives a posting's languages as the ones it requires, else the one it is written in.
- **Pay.** The model's `salary` is `{min, max, currency, period}`; the top (max, else min) is
  converted with `EUR_PER_UNIT` (EUR 1, USD 0.92, GBP 1.17, CHF 1.05) and 12 months a year.
  Same currency as the floor compares without the table. Another currency outside the table,
  or a week, day or hour period, passes with a gap ("Pay stated in … not compared with your
  floor"). Gaps are now up to 3 (two from the model, one from the rules).
- **Reasons and counts.** The first rule that fails is the reason, in the table's order
  (place, language, pay). A search's `dropped` counts the jobs it scored (now or from the
  cache) that fail a rule, whatever their score; the search line says "… 9 matched, 14 dropped
  for your requirements" when it is above zero. `sources.scores` keeps `excluded_reason`.
- **Pre-filter.** Jobs carry `board_remote` (Ashby `workplaceType`, else `isRemote`; Lever
  `workplaceType`; none elsewhere): `false` is dropped before the model for a remote-only
  person (`Out::NotRemote`). The pre-rank adds 0.15 to a job whose structured location names
  remote, Europe, EMEA, EU, anywhere, one of their places or the country they live in (the
  plan's "Portugal", generalised to the person's own country).
- **Score cache.** `rules_version` (1) joins the cache key; rows from before (version 0) are
  never read, so each job is asked once more with the facts. `MAX_TOKENS` per call is 8 000
  (was 6 000) for the facts; the excerpt stays 1 500 characters.
- **Recheck.** `sources.recheck_requirements` is enqueued at every server start (unique: a
  waiting one is not doubled), takes Found/Shortlisted opportunities that a search added (a
  match, not `link`), not dismissed, with no facts yet (`remote IS NULL`), newest first, at
  most 30 a run; per person within their limits; sets the facts and `hidden_reason` only while
  the row is still Found/Shortlisted and unchecked. What it could not check waits for the next
  start. The pipeline reaches it through two new ports (`unchecked`, `set_requirements`).
- **API.** `GET /api/v1/opportunities?hidden=true` lists only the hidden ones (otherwise they
  are left out, and never counted as new on Today); summaries carry `hidden_reason` and
  `region_note`, an opportunity carries `facts` (`JobFacts`) and `hidden_reason`; a search
  carries `dropped`.
- **Client.** The stage filter's last option is "Hidden: don't meet your requirements"
  (`pipeline-stage-filter` value `hidden`): a banner (`pipeline-hidden-only`, back with
  `pipeline-show-in-view`) and a "Why it's hidden" column or card line
  (`pipeline-row-hidden-reason`). The peek shows "Your requirements" (`peek-facts`:
  "Remote · Open to where you live (EU) · English") and, when hidden, a banner (`peek-hidden`).
