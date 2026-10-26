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
`GET /api/v1/goals` → `{ version, answers }`; `PUT /api/v1/goals` (whole answers, validated) → new version. CSRF on PUT.

## Client
- Interview route `/goals/interview?step=<key>`: one question per view, Back / Skip / Next, progress dots, Esc or "Finish later" leaves; answers save as you go (`PUT` per step). Starts after the first profile import (offered, not forced) and from the Goals section.
- Goals section on Profile & goals: each answer as a Notion-style property row, inline edit with the same controls as the interview.
- 375px + desktop, 4 languages, keyboard: Enter = Next, Shift+Enter in lists adds an item.
