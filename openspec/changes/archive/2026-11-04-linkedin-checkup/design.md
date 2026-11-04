# Design: the LinkedIn check-up

## Where it lives
A slice of the profile module, `features/linkedin/`: the subject is the person's profile and goals,
and the slice reuses what the module already has: the import's readers (`imports::linkedin::read_export`
for the ZIP, `imports::pdf::text_of` and `imports::structure::structure` for the PDF), the sources
Tell me about you cuts the profile and goals into (`about::prompt::sources_of`, which carries no name
or contact detail), its contact scrub (`about::prompt::without_contact`), and the kernel's claim check
(`applicamp_kernel::claims::Evidence`). No port, no event: everything it reads is the module's own.

## Data (the profile schema owns it)
`profile.linkedin_reviews`, one row per account (`account_id` primary key): `kind` (`linkedin_zip` or
`pdf`), `language`, `profile_version` and `goals_version` (what the review was made against),
`linkedin` (the LinkedIn profile as read, a `Profile` without basics' name, email, location, url or
profiles, every free text passed through the contact scrub), `overall`, `findings` and `proposals`
(JSON, with each proposal's sources, flags, status, text and whether it was edited), `created_at`,
`updated_at`, `expires_at` (30 days after the last touch; `profile.expire_imports` deletes expired
rows, as it does for drafts and Tell me about you). `profile.linkedin_calls` counts check-ups for the
daily cap. Nothing is published or consumed.

## Endpoints (scope: the signed-in session, own account only; anyone else's is 404)
- `GET /api/v1/profile/linkedin`: the state (`LinkedInView`: `has_profile`, `ai_available`, the
  review if any with `stale`, `checkups_today`, `daily_cap`). Never calls a model.
- `POST /api/v1/profile/linkedin?locale=` (multipart `kind`, `file`): reads the file, writes the
  review (one model call; three for a PDF), replaces the account's review, answers 201 with the
  state. 409 `profile_needed` without a profile; 429 `linkedin_daily_cap` over the cap; the import's
  upload errors (400 `invalid_upload`, `file_too_large`, `not_a_zip`, `archive_*`, `not_a_pdf`,
  `pdf_*`, `nothing_found`); 503 `ai_unavailable`, `ai_daily_limit_reached`, `ai_budget_reached`,
  `ai_failed`.
- `PATCH /api/v1/profile/linkedin/proposals/{id}` (`status`: `ready` | `kept` | `dropped` | `done`,
  and/or `text`): a decision, or an edit checked again against the proposal's sources. 404 for an
  unknown id; 409 `proposal_flagged` on `kept` while flags remain; 409 `proposal_not_kept` on `done`
  from anything but `kept`; 400 `invalid_text` outside 1 to the field's limit.
- `DELETE /api/v1/profile/linkedin`: Start over; 204.

## The prompt (`complete` role, temperature 0, one call, JSON schema answer)
`You review one person's LinkedIn profile for a job-hunting app…` (`INSTRUCTIONS`, version 1). The
message holds `<language>`, `<goals>` (roles, seniority, work mode, company stage, must-haves, what
to avoid, languages for work; never the cash floor or where they live), `<sources>` (the profile and
goals as Tell me about you shows them, ids `p.headline`, `w0`, `w0f1`, `skills`…, plus the LinkedIn
profile's own parts as sources `l.headline`, `l.summary`, `l0`… so a proposal may keep what the person
already published) and `<linkedin>` (the LinkedIn profile as JSON, positions with their index).
Everything shown is data (angle brackets made plain). The answer is `{overall, findings: [{part,
position, state, why, proposals: [{field, text, sources, why}]}]}`. Budgets 6k then 10k tokens; one
retry after a cut-off; a usable answer or `ai_failed`. The model never sees a name, an email, a phone
number, an address or a link; a test asserts it.

## The judge (no model; flag, never drop)
Findings: at most one per part (a position by its index into the LinkedIn profile), unknown parts and
indexes left out, `why` cut to 300 characters. Proposals: at most 16; one headline (220 characters,
LinkedIn's limit), one About (2,600), one description per position (2,000), skills one per proposal
(100), a skill already on LinkedIn or proposed twice left out. Each proposal keeps the cited ids that
exist; its evidence is their texts; every claim (`claims`) not in the evidence is a `claim` flag; every
buzzword from a fixed list (en/pt/es/fr) not in the evidence is a `buzzword` flag. A proposal with no
valid source passes only if it claims nothing. An edit is cut to the field's limit and checked the same
way; a kept or done proposal that is edited goes back to `ready`.

## Who may do what, and undo
The person clicks every model call (Check my LinkedIn, Check again). Keep, Drop and Done each can be
taken back (`ready`). Start over deletes the review. Nothing is written to the profile or the goals;
nothing leaves the app. Without a configured model, `ai_available` is false, the page says the
check-up needs one, and POST answers 503.

## Client
`LinkedInPage` at `/profile/linkedin` in the profile module (`components/LinkedIn/`): the upload
step (ChoiceCards for the ZIP or the PDF, the import's own words on where each comes from, FileField,
"Check my LinkedIn · 1 AI call · about €0.003"), the review (the overall read; a stale banner; one
block per part with its state as a tag, why, what LinkedIn says now, and its proposals, each with its
sources under "Where this came from", its flags, Keep, Edit, Drop or its state), then "To change on
LinkedIn" (the kept proposals, each a CopyField and Done), and Check again / Start over.
`LinkedInSection` on Profile & goals, below the strength. Strings in `profile` under `linkedin.*`, in
four languages; no shortcut hints; everyday words.
