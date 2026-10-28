# Profile completion: always working toward a strong profile

Issue: https://github.com/notacamp/applicamp/issues/24

## Why
Owner, 2026-10-26: "the process should use AI to try to get as complete a
profile as possible about me. It's goal should be always to achieve
completion and build a strong profile." The owner's own profile came out of a
PDF import with 13 positions and nothing else. Prototype approved 2026-10-28:
https://claude.ai/artifact/Jk5QokoGi12rZqw8EhWLw9

## What changes
- **Profile strength**: a score and checklist computed in code from the
  profile and goals (headline, summary, achievements with results on the most
  recent roles, skills, education, languages, links, positions, goals), on
  Profile & goals and on Today.
- **Improve my profile**: the AI reads the profile once per version and
  proposes up to 9 targeted questions (a role without results, no headline,
  no summary…), each with why it matters and the expected gain. One question
  per screen, skippable, finish later anytime.
- **Draft from my answer**: the AI turns the person's answer into achievement
  bullets / a headline / a summary. Drafts may only use facts from the answer
  and the profile (a draft source check); Keep / Edit / Discard; nothing is
  saved until Keep.
- **Skills found in your roles**: skills that the person's own position
  descriptions mention but the skills list doesn't, with the number of roles,
  each Add / Not mine. The Skills page also lets the person put their skills in
  order (move up, move down): the first three drive the job board searches.
- **Today** shows the profile strength and "Continue" while the profile isn't
  strong; "Not now" hides it for a week.

## Modules touched
`profile`. Client: `modules/profile`, Today.

## Non-goals
- Tailored CVs (#11) — they will consume the stronger profile.
- Rewriting existing positions wholesale; drafts only add.
