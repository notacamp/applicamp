# Hard requirements are pass/fail, not part of the score

Issue: https://github.com/notacamp/applicamp/issues/19

## Why
The owner set "remote only", Lisbon, no relocation; the first searches kept
"remote-first from Germany or Austria" (score 85, with that fact listed as a
gap), a German-language posting and a role whose remote status was unknown.
Goals that are requirements were only an input to the model's score.

## What changes
- The model reports **eligibility facts** for each job instead of weighing
  them into the score: `remote` (yes / no / unclear), `open_to_candidate`
  (can someone living in the person's location do it without moving: yes /
  no / unclear), `working_languages` (languages the job requires), `salary`
  (stated min/max/currency/period, or none).
- **Code decides** with fixed rules from the goals; a job that fails is never
  added, whatever its score. Unclear counts as failing for a requirement the
  person marked.
- Cheap pre-filter on structured board fields (Ashby `isRemote` /
  `workplaceType`, Lever `workplaceType`, Greenhouse and Arbeitnow location
  names) drops what is clearly on-site or hybrid before any model call.
- Opportunities already in the pipeline are re-checked once; the ones that
  fail are hidden (not deleted) with the reason, viewable under
  "Hidden: don't meet your requirements".

## Modules touched
`sources` (filter, scoring, rules), `pipeline` (hidden reason), client Pipeline.

## Non-goals
- New goals questions (the interview stays as it is).
- Sandboxed runs.
