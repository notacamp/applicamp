# Decisions needed

Open questions for the owner, one per heading, each with a recommended answer.

## Remotive: its robots.txt disallows the API its terms invite (#5)

Remotive's API terms (remotive.com/api-documentation, and the notice in every answer) invite
developers to share its jobs with a link back, at most 4 fetches a day. Its robots.txt
(read 2026-10-27) disallows `/api/*` and `/*category=` for every crawler. The board check
honours robots.txt, so Remotive is seeded as a candidate it cannot approve.

**Recommended:** keep it blocked, and email Remotive (the contact on its API page)
asking whether the documented API is meant to be exempt from those robots rules. If they say
yes in writing, approve it with a short note in its terms; if not, decline it with that reason.

## Who owns the job boards on the hosted instance (#5)

Approving, declining, pausing and editing boards is limited to the addresses in
`APPLICAMP_SOURCES__BOARD_OWNERS`; with none set, nobody can approve a board from the app (the
seeded approved boards are still read).

**Recommended:** set it to the owner's sign-in address in the hosted instance's configuration
(applicamp-cloud), and nobody else's.

## Do hosted ATS forms allow a candidate's own approved submission? (#41, #7)

Every ATS apply API (Greenhouse, Lever, Ashby, SmartRecruiters) needs a key only the employer
holds, so Applicamp can only apply through the employer's hosted form in a browser. We found no
candidate-facing terms on those forms that allow or forbid an approved, browser-driven
submission made for the candidate.

**Recommended:** read the candidate-facing terms of Greenhouse, Lever and Ashby hosted forms
before applying ships. Until then, applying prepares the form and hands it to you to submit, and
the scripted submit stays behind a setting that is off.

## Is the `complete` model good enough at tool calls? (#41)

OpenRouter reports tool-call error rates of about 8% (before its routing fix) and about 1%
(after) for GLM models; there is no figure for the model the hosted instance uses.

**Recommended:** check that the model lists `tools` among its supported parameters, then
measure pass^3 on the agent evals. Below 90%, run the agent loop on a stronger model and keep
the current one for single structured calls.

## Default limits on applications (#41, #7)

**Recommended:** at most 10 submissions a week, one per company and role every 90 days, 30
minutes between submissions, and a fit score of at least 70 before an application is
prepared. A person may lower these, never raise them past the instance's ceiling.

## How long to keep run screenshots (#41)

**Recommended:** take screenshots only just before a submit and on an error, store them
privately per run, and delete them 30 days after the opportunity closes.

## Legal review before opening sign-ups (#41)

The EU AI Act's Annex III 4(a) is about recruitment AI used by employers; a tool a candidate uses
for themselves is probably outside it, but no guidance says so. Recruiter emails hold other
people's personal data.

**Recommended:** have a lawyer review the GDPR position and AI disclosure before the hosted
instance accepts people outside the invite list.
