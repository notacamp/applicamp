-- Every CV draft is a PDF you can see (#73).
--
-- The draft's preview: its text rendered as a PDF as soon as the draft is made
-- and again after every change, with the layout it fits (Clean on one page,
-- else two pages, else Compact on two). The read-back's results are kept as a
-- note for the person: a preview is never refused for them (approving still
-- is). A render that fails keeps its short code, never the CV's text.
-- Stored with the account's data like the versions, never in a public bucket,
-- and cleared when the draft is approved or put away.

ALTER TABLE pipeline.cv_drafts
    ADD COLUMN IF NOT EXISTS preview_pdf bytea,
    ADD COLUMN IF NOT EXISTS preview_template text
        CHECK (preview_template IS NULL OR preview_template IN ('clean', 'compact')),
    ADD COLUMN IF NOT EXISTS preview_length smallint
        CHECK (preview_length IS NULL OR preview_length IN (1, 2)),
    ADD COLUMN IF NOT EXISTS preview_pages smallint
        CHECK (preview_pages IS NULL OR preview_pages >= 1),
    ADD COLUMN IF NOT EXISTS preview_checks jsonb,
    -- SHA-256 of the draft's text the PDF was made from: Approve keeps the
    -- PDF only when it is still that text.
    ADD COLUMN IF NOT EXISTS preview_of bytea,
    -- `cv_doesnt_fit` or `cv_pdf_unavailable` when the last render failed.
    ADD COLUMN IF NOT EXISTS preview_error text
        CHECK (preview_error IS NULL OR char_length(preview_error) <= 64),
    ADD COLUMN IF NOT EXISTS preview_at timestamptz;

-- Each "see it in another layout" or "Make the PDF" the person asks for: at
-- most 30 a day per account (`cv_previews_daily_cap`). The ones made after a
-- change are not counted. Ids and times only.
CREATE TABLE IF NOT EXISTS pipeline.cv_preview_makes (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    made_at timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS cv_preview_makes_account_made
    ON pipeline.cv_preview_makes (account_id, made_at);
