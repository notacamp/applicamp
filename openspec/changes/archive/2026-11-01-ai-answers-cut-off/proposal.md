# Searches no longer stall on cut-off answers

Issue: https://github.com/notacamp/applicamp/issues/53

## Why
Since 10-27 every search on the hosted instance ended `ai_failed`, scoring 0 to 20 of about 400
candidates, so the nightly searches added nothing. Measured against the live provider: the model
is served by many providers on OpenRouter, and some think for thousands of tokens by default (one
spent 5,788 reasoning tokens and 244 s on one batch of ten), so a batch ran out of room before or
during its JSON answer. One failed batch then ended the whole search. The job brief's company
facts were cut off the same way.

## What changes
- **The AI gateway, on OpenRouter, asks for low reasoning effort** (`reasoning.effort: low`) and
  **routes only to providers that support every parameter sent** (`provider.require_parameters`),
  the answer's JSON shape included. Both are settings (`APPLICAMP_AI__COMPLETE__REASONING_EFFORT`,
  `…__REQUIRE_PARAMETERS`); neither is sent to other providers unless set. Measured on the same
  batch: 5,788 reasoning tokens became 132, and every answer parsed.
- **Each call's log line says which provider answered and how many tokens it spent thinking**, so
  the next cut-off is found in minutes (ids and counts only, as before).
- **A batch whose answer is unusable is scored again as two halves**; a half that fails too waits
  for the next search. Only four failed calls in a row (the provider is down) end the scoring.

## Not changing
- The best 40 candidates per search, ten a call, the limits and the prompt.
