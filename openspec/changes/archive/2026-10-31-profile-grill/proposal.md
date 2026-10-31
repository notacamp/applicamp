# Grill me: a deep interview about each role

Issue: https://github.com/notacamp/applicamp/issues/33 (part of #11, tailored CVs)

## Why
A CV can only say what Applicamp knows, and a profile imported from a PDF
says little about what the person actually did in each role. Improve my
profile (#24) asks a handful of gap questions across the whole profile. To
write a CV that holds up in an interview, each role needs a deeper, ordered
interview: what the company did, what the person did themselves, the scale,
results with proof, hard problems, people, and stories. Prototype approved
2026-10-30 (tab "1 · Grill me"): https://claude.ai/artifact/KmztFvd4e7P3uiWSvbUSX5

## What changes
- **Grill me**, from Profile & goals: pick a role and an interviewer asks one
  question at a time, in the person's language. It pushes back on vague
  answers ("went well"), asks for numbers and how the person knows them, asks
  what they did themselves as against the team, and asks which answer is
  right when two disagree or one contradicts the profile. It works through
  seven areas in order (context, your part, stack and scale, results, hard
  problems, people, stories) and aims for about 12 questions per role.
- **Facts you confirm.** Each answer yields short facts, each shown with the
  person's exact words and Keep / Fix / Drop (and Keep all). A kept fact is
  saved on that role in the profile document (`work[].facts`), so every later
  check and the CV fact check treat it as the person's own words. Nothing is
  saved without a click.
- **Coverage** per role: seven rows (done, half, not yet) and n/7, from the
  list of roles (not started, in progress, covered) and on the chat.
- **Kept facts on the profile**, under each role, with Remove and Undo.
- Questions can be skipped, the role can be left with "Done for now", and
  it continues later.

## Modules touched
`profile` (server: new slice `features/grill`, schema `profile`; client:
`modules/profile`, a new route and entry points) and the fake model in
`clients/app/scripts/fake-jobs.ts`.

## Cost
AI role `complete`, temperature 0, no background calls. **One call** per
start, answer or skip, about 3k tokens in and 400 out, so about
EUR 0.0005 to 0.001 a call. A role takes about 12 questions, so roughly
EUR 0.006 to 0.012 a role. Starting again with a question waiting, keeping,
fixing, dropping, "Done for now" and every read make no call. A skip with
nothing left to ask makes no call.

Every call goes through the existing budget, which reserves its worst case
before it is sent: EUR 0.30 a day and EUR 5 a month per account, EUR 15 a
month for the instance. Over a limit the call is refused before it is sent,
the page says `ai_budget_reached` in plain words, and nothing is retried
until the person asks again.

A new hard cap on top: **150 Grill me calls per account per UTC day**
(problem `grill_daily_cap`, HTTP 429), counted from the interview's own
turns. It limits a runaway loop, not a normal day: 150 calls is about 12
roles of 12 questions.

## Non-goals
- The tailored CV itself, company and job facts (#11's later changes). They
  will read the facts this change keeps.
- Any call in the background, and any change to what Improve my profile asks.
- Voice or file answers: text only, 2000 characters at most.
- Re-asking questions the person skipped, until they start the role again.
