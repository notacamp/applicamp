# Design

## The rule (`crates/modules/sources/src/grounding.rs`)
The **source** is what the model was shown about the person: every string and
number in the profile summary and the goals, plus where they live (never JSON
keys). It is compared the way the import source check compares: lower case, no
whitespace, plain quotes and dashes.

A **claim** is a word of a reason (split on whitespace, edge punctuation
trimmed, hyphenated and slashed words split into parts) that is any of:
1. a known technology name (`applicamp_kernel::tech`, any case: `rails`, `TypeScript`, `c++`);
2. a word containing a digit (a number of years, a year, a version, `60k`);
3. a word starting with a capital letter that is not an ordinary word (a small
   list of generic words: `Remote`, `Senior`, `Engineer`, `Matches`…), which
   covers frameworks, companies, titles and places. The first word of a reason
   is capitalised by grammar, so it counts under 3 only as a technology name or number.

A claim is **supported** when the source says it: a number matches as a whole
number (`5` is not in `2015`; `60k` also matches `60000`); a word of up to three
letters matches as a whole word (`Go` is not in `Google`); anything longer
matches as a substring of the squashed source. A reason with any unsupported
claim is **dropped**; one with none is kept. Gaps are never checked.

## All reasons dropped
When the model gave at least one reason and every one was dropped, nothing the
person wrote supports the match, and the eligibility facts (remote, location,
language, pay) say nothing about fit. The match is capped at `KEEP - 1` (64),
so the job is not added, and the score is cached `ungrounded = true`. The search
row counts these jobs in `not_grounded` (and the log line). A job the model
gave no reasons for at all is left as scored: there is nothing to ground.
When some reasons survive, the score stands on them.

## Prompt
The system rules say a reason may name a technology, framework, language,
company, title, place or number only when `<profile>` or `<goals>` contains it,
never to infer the stack from the job or the wanted role, and to give fewer
reasons, or none, when little is written. A new `<not_in_profile>` block lists,
from the profile summary: "No skills are listed. Do not infer a technology
stack." (empty skills), "The profile and goals name no technology, language or
framework.", "No positions are listed." and "There is no headline."

## Cache and cost
`RULES_VERSION` 1 → 2. Scores are keyed by account, profile version, goals
version and rules version, so every cached score is asked again, once per job,
on the next search or nightly run, within the usual spending limits. The cost is
one scoring pass (about one call per ten jobs) per account. `sources.scores` and
`sources.searches` gain `ungrounded` / `not_grounded`.

## One-off for what is already in the pipeline
`sources.reground_reasons` is enqueued at every start (once; a waiting one is
not doubled), like the requirements recheck. It pages through Found and
Shortlisted opportunities found by a search whose `reasons_checked` is false
(keyset by id), builds the owner's source from their current profile and goals,
and for each opportunity keeps the supported reasons, sets `reasons_checked`
and `reasons_updated` (true only when something was stripped). No model call,
no cost. Never hidden, deleted, re-staged or re-scored; later stages are the
person's own choice and untouched. Someone whose profile is gone waits for a
later start. New rows come checked (`reasons_checked` defaults to true).

## API and client
`Opportunity.reasons_updated` (bool): the peek shows "Reasons updated: the ones
your profile doesn't support were removed." `Today.profile.no_skills` (bool):
the profile lists no skills and the summary a search sends names no technology
(`profile::matching::no_stack`). The client shows the "no skills" line under the
Pipeline's search line and on Today (replacing the generic add-skills hint),
linking to `/profile#skills`; Skills on Profile & goals is anchored.
