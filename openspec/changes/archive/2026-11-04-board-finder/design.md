# Design

- **Data** (`sources.job_boards`, migration `20261104100000_sources__board_finder`):
  `approved_by` becomes text (`owner` or `app`; it held the approving account's id),
  `approved_at` stays; new `tags text[]`, `terms_verdict`, `terms_quote`, `terms_url`,
  `content_signal jsonb`, `contact_email`, `found_through`. All appear on `BoardSummary`.
  `report.terms_read_for` holds the fingerprint of the terms sentences a verdict is from: the
  cache key.
- **Blockers are computed**: `needs_permission` and a model's `terms_forbid` come from the stored
  verdict (`terms::blocker_of`), so the owner clearing the verdict (the permission flow of #86)
  lifts them. Seed lists may state `needs_permission` as a fact with a quote and a source.
- **Clean check** is `is_a_live_job_board` plus `blockers(row)` empty plus a fresh verdict of
  allow or silent, evaluated after the check is stored, and applied with a guarded `UPDATE ...
  WHERE status = 'candidate' AND config_edited = false` so an owner decision in between wins.
- **Fresh verdict**: the terms sentences' fingerprint equals `terms_read_for`, or the homepage was
  read and links no terms page (silent). A terms page that could not be reached keeps the old
  verdict and is not fresh, so a flaky fetch never turns a forbid into a silent.
- **Model call**: `complete` role through `applicamp-ai`, `Caller` with the nil account id (the
  system's), so the reserve-then-settle ledger and the instance monthly limit apply; the
  account day limit and the daily token cap apply to that account too. Over a limit the call is
  not sent, the check records a flag, and the run stops asking.
- **Quote must be one of the sentences**: an answer that forbids or asks first with a quote that
  is not in the input is discarded (no verdict stored).
- **Lists** (`features/discovery/lists.rs`): a table of sources with id, credit, licence, URL,
  format and the headings to read; a feed address in a list becomes the candidate's config
  endpoint so the check reads it first.
