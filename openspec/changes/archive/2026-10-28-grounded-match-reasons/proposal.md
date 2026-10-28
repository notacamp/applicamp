# Grounded match reasons: no invented stack

Issue: https://github.com/notacamp/applicamp/issues/27

## Why
Owner, 2026-10-28: "why my profile has been labeled more as typescript dev
when in ruby/rails/react dev mostly". In production the profile had no
headline, no skills and no position text naming TypeScript, Ruby, Rails or
React, yet matches said "Full-stack TypeScript matches profile". The scoring
model invented the person's stack, and a wrong reason is worse than none: it
teaches the person to distrust every match and, once CVs are tailored from
matches, it would put claims in their name.

## What changes
- **Reason source check.** Every technology, framework, language, company,
  job title, place and number that a `why` reason attributes to the person
  must appear in the profile or goals text the model was shown. A reason
  that fails is dropped. If every reason is dropped the match is held below
  the keep threshold (so it is not added) and counted as "not grounded".
- **Prompt.** It states what the profile does not contain ("No skills are
  listed. Do not infer a technology stack."), requires reasons to cite
  written facts, and leaves gaps free to name the posting's requirements.
- **Score cache.** The rules version moves from 1 to 2, so every cached
  score (and its reasons) is asked again, once per job, on the next search.
- **One-off at deploy.** Found and Shortlisted opportunities found before
  the check have their reasons checked once, with no model call: the ones
  the profile never supported are stripped and the peek says "Reasons
  updated". Nothing is hidden or deleted.
- **No stack.** When the profile lists no skills and names no technology,
  the Pipeline's search line and Today say "Your profile lists no skills
  yet. Add them for better matches", linking to Skills on Profile & goals.

## Modules touched
`sources` (grounding, scoring, search, one-off), `pipeline` (store, API:
`reasons_updated`, Today `no_skills`), `profile` (`no_stack`), `kernel`
(technology names). Client: Pipeline, Today, peek, Profile anchor.

## Non-goals
- Judging whether a reason is persuasive; only whether it is grounded.
- Checking gaps: they come from the posting.
