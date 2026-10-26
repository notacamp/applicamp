# Design

## Data
`profile.goals(account_id pk, version int not null, answers jsonb not null, updated_at)`; every save bumps `version`. Event `profile.goals_updated` v1 `{ account, version }`.

Answers (all optional; typed in `crates/modules/profile/src/goals.rs`):
| Key | Type | Question (en) |
|---|---|---|
| `roles` | list of text (≤ 5) | What roles do you want next? |
| `seniority` | one of `junior, mid, senior, staff, principal, lead, manager, director, founding` (multi) | Which levels fit? |
| `work_mode` | multi of `remote, hybrid, onsite` | How do you want to work? |
| `locations` | list of text (cities/countries/regions, ≤ 10) | Where can you work from? |
| `relocate` | `no, within_country, within_eu, anywhere` | Would you move for the right job? |
| `cash_floor` | `{ amount int, currency ISO 4217, period: year|month }` | What's the least you'd take, before tax? |
| `equity_counts` | bool | Does equity count toward that? |
| `company_stage` | multi of `early, growth, established, public, nonprofit` | What kind of company? |
| `avoid` | list of text (≤ 10) | Anything you won't work on? |
| `must_have` | list of text (≤ 10) | What must the job have? |
| `languages_for_work` | list of BCP 47 tags | Which languages can you work in? |
| `autonomy` | `find_only, prepare, prepare_and_schedule` | How much should agents do alone? (applying and emailing always wait for you) |

## Endpoints
`GET /api/v1/goals` → `{ version, answers, updated_at }` (version 0 and no answers until something is saved); `PUT /api/v1/goals` (whole answers, trimmed, repeats and blanks removed, validated; `400 invalid_goals`) → new version, every save even of the same answers. CSRF on PUT. Operation ids `profile.get_goals` and `profile.update_goals`. Currency is checked against the active ISO 4217 codes; language tags against the BCP 47 shape; lists against their limits (5 roles, 10 places, avoid and must-haves, 100 characters an item).

## Client
- Interview route `/goals/interview?step=<key>` (signed in, outside the app frame): one question per view, Back / Skip / Next, a progress bar and "Question n of N", Esc or "Finish later" leaves for Today; answers save as you go (`PUT` per step, only when the answer changed). Past the last question it leaves for Profile & goals. Equity is only asked once there is a cash floor. Offered after a profile import is applied, and started or continued from the Goals section.
- Goals section on Profile & goals: each answer as a Notion-style property row, inline edit with the same controls as the interview (`GoalQuestion`). New lib/ui compositions: `ListField` (a short list built item by item) and `InlineEdit` (any controls as an in-place editor).
- 375px + desktop, 4 languages, keyboard: Enter = Next, Shift+Enter in lists adds an item.
