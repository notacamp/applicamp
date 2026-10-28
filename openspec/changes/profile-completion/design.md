# Design

## Strength (code, `crates/modules/profile/src/strength.rs`)
Weights (sum 100): headline 6 · summary 8 · achievements 24 (the 3 most recent roles: each with ≥ 1 highlight = 6, a highlight with a number or % = +2) · skills 12 (≥ 5 confirmed = full, proportional below) · education 6 · languages 6 · links 4 · positions 16 (≥ 1 = 8, ≥ 3 with dates = 16) · goals 18 (proportional to answered). Strong = ≥ 80. Returned with each item's state (`ok|partial|missing`), a one-line hint and the target action (`question:<id>`, `skills`, `import`, `edit:<section>`). Unit-tested table.

## Gap review (one model call per profile+goals version)
`complete` role, input: the profile (compact JSON) + goals + strength items; output (strict JSON): `questions: [{ id, section (work|basics.label|basics.summary|education|links|other), role_ref? (index of the position), question (≤ 160 chars), why (≤ 140), expected_gain (int) }]` (≤ 9, ordered by gain) and `skills_in_roles: [{ name, roles: [indices] }]` (≤ 20). Code validates: role_ref exists; every skill name appears (case-insensitive, word boundary) in the referenced positions' text, else dropped; counts recomputed in code. Stored in `profile.gap_reviews(account_id, profile_version, goals_version, questions jsonb, skills jsonb, created_at)`; reused until the profile or goals change. Spend limits apply; if `complete` is unavailable, Improve shows only code-derived items (strength checklist + skills with zero questions).

## Draft from an answer
`POST /api/v1/profile/questions/{id}/draft { answer (≤ 2000 chars) }` → `complete` call with the question, the answer and the relevant profile part → `{ kind: highlights|label|summary|education|link, items: [string] }`. **Draft source check**: every number, percentage, currency amount, proper noun and technology name in a draft item must appear in the answer or the profile (same normalisation as the import source check); items that fail are dropped and counted ("1 line left out: it mentioned something you didn't write"). Drafts live 24 h (`profile.drafts`). `POST …/drafts/{id}/keep { items (possibly edited) }` writes into the profile (role highlights / basics.label / basics.summary / education / profiles) as a new profile version with source `answer`; edited items are the person's own words and skip the check. Discard deletes. Skip marks the question skipped (not asked again for this version).

## Skills
`GET /api/v1/profile/skills/suggested` (from the gap review) → Add appends `{name}` to `skills`; Not mine records it so it is not suggested again.

## Endpoints (session, OpenAPI)
`GET /api/v1/profile/strength`, `GET /api/v1/profile/questions` (runs the gap review if missing; 202 while running), `POST …/questions/{id}/draft`, `POST …/drafts/{id}/keep`, `POST …/drafts/{id}/discard`, `POST …/questions/{id}/skip`, `GET …/skills/suggested`, `POST …/skills/suggested/{name}/{add|dismiss}`. `GET /api/v1/today` gains `profile_strength` and `next_profile_action`.

## Client (prototype)
Profile & goals: strength card (score, bar, checklist with Add/Improve per item). Improve route `/profile/improve?q=<id>`: context chip, question, why, answer box, Draft from my answer → draft block (Keep / Edit inline / Discard), Skip, Finish later, progress "Question n of N · strength a% → b%". Skills page `/profile/skills`: chips with "in N roles", Add / Not mine. Today: strength card while < 80 with Continue / Not now (hidden 7 days). Keyboard: Enter in the answer box = Draft (Shift+Enter newline), Esc = Finish later. 4 languages; 375 + desktop.

## Cost
Gap review: 1 call per profile/goals version (~2–4k tokens in, ~1k out). Each draft: 1 small call. All within the spend limits; estimates not shown (each call is tiny), but `ai_budget_reached` is handled with a clear message.
