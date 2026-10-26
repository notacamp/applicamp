# Design

## Data (schema `profile`)
- `profiles(account_id pk, data jsonb not null, updated_at, source text)`. `data` follows `crates/modules/profile/src/schema.rs` (a typed Rust struct serialized as JSON; JSON Resume field names where they exist: `basics`, `work`, `education`, `skills`, `languages`, `certificates`, `projects`).
- `imports(id uuid pk, account_id, kind text check in ('linkedin_zip','pdf','text'), status text check in ('ready','failed','applied','discarded'), draft jsonb, dropped int, created_at, expires_at)`. A draft lives 24 hours; a job deletes expired ones. No file contents are stored, only the structured draft.
- Events: `profile.updated` v1 `{ account }` (ids only).

## Endpoints (session; OpenAPI; problem+json)
- `POST /api/v1/profile/imports` multipart (`kind`, `file` or `text`) → `201 { import }`. Limits: ZIP 20 MB, PDF 8 MB, text 60 000 chars.
- `GET /api/v1/profile/imports/{id}` → the draft (404 if not yours).
- `POST /api/v1/profile/imports/{id}/apply` with `{ mode: "replace" | "merge" }` → the profile. `merge` adds positions/education/skills not already present (same title+company+start, same name) and fills empty fields. *As built:* a position is "already present" when the company matches and so does its start **or** its title, so a title (or a start) edited by hand never brings the imported one back beside it; a school by name + degree.
- `POST /api/v1/profile/imports/{id}/discard`.
- `GET /api/v1/profile`, `PUT /api/v1/profile` (whole document, validated). *As built:* both answer `{ profile, source, updated_at, import_kinds }`; `import_kinds` is `["linkedin_zip"]` without the `complete` role and adds `pdf`, `text` with it, which is how the client knows to offer only the ZIP.

## LinkedIn export ZIP
- Accept only `.zip`; refuse archives with more than 200 entries, any entry over 5 MB uncompressed, total over 50 MB uncompressed, or paths with `..`/absolute paths (zip-slip). Read only known CSVs, matched case-insensitively by file name: `Profile.csv`, `Positions.csv`, `Education.csv`, `Skills.csv`, `Languages.csv`, `Certifications.csv`, `Projects.csv`, `Email Addresses.csv` (primary only, not stored if it differs from the account's? stored as `basics.email`). Columns are matched by header name, tolerant of order and extra columns; dates like `Jan 2020` / `2020` become `2020-01` / `2020`. Unknown files are ignored.
- No model call.

## PDF and text
- PDF text via a pure-Rust extractor (`pdf-extract` or `lopdf`), max 30 pages; an empty result (scanned PDF) fails with "This PDF has no text we can read".
- Structuring: `complete` role, one call, system prompt + the text, asking for JSON matching `schema.rs` (JSON schema derived with `schemars`). Untrusted content: the text is data; the prompt says so and the output is parsed strictly.
- **Source check**: every string value (titles, companies, schools, skill names, dates) must appear in the source text after normalizing whitespace and case (dates also in common written forms); values that don't are dropped and counted in `dropped`, shown to the user ("3 details weren't in the file, so we left them out").
- Fallback: when `complete` is not configured, PDF and text imports answer `503 ai_unavailable` and the UI offers the ZIP import only.

## AI gateway (`crates/ai`)
*As built:* the ledger's `ai` schema is infrastructure like `kernel` and `bff` (`cargo xtask arch` knows `crates/ai` and lets modules depend on it); a draft's expiry is a job enqueued for its `expires_at` (`kernel::jobs::enqueue_at`). `basics.location` is one written place (`"Lisbon, Portugal"`), not JSON Resume's address object.

Trim Campbooks Next's `crates/ai` to: the `complete` role (OpenAI-compatible chat completions), timeouts, retries on 429/5xx with backoff, a usage ledger table `ai.usage(account_id, role, model, tokens_in, tokens_out, created_at)` and a per-account daily token cap (`APPLICAMP_AI__DAILY_TOKENS`, default 200 000). Config `APPLICAMP_AI__COMPLETE__{BASE_URL,MODEL,API_KEY(_FILE)}`. Logs never carry prompts or outputs.

## Client
- Profile & goals page: when empty, the import panel (three choices); when filled, the profile as Notion-style properties and lists with inline edit (save = `PUT`), and "Import again".
- Import review: the draft side by side with the current profile (new / changed / same), the dropped count, Replace or Merge, Discard. Nothing is saved before this.
- Today's "Set up your profile" goes to the import panel. 375px + desktop, 4 languages.

## Security
Files never reach disk outside the request (in-memory with limits); zip-slip and zip-bomb guards; 404 for others' imports; CSRF on every POST/PUT; no CV content in logs, events or metrics.
