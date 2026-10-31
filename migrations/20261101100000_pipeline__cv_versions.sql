-- Tailored CVs, part B (#38): what was approved is kept as a PDF.
--
-- A version is the approved text rendered with a template and a length,
-- read back out of the PDF and found to say what the CV says. It is never
-- overwritten: a new template or length is a new row, with the same approved
-- text and no AI call. The PDF is stored here, with the account's data and
-- its backups, and never in a public bucket (about 30-50 KB each).

CREATE TABLE IF NOT EXISTS pipeline.cv_versions (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    -- The approved draft whose text it renders.
    draft_id uuid NOT NULL REFERENCES pipeline.cv_drafts (id) ON DELETE CASCADE,
    -- Its number for the opportunity, from 1; never reused.
    version integer NOT NULL CHECK (version >= 1),
    template text NOT NULL CHECK (template IN ('clean', 'compact')),
    -- The most pages it may take.
    length smallint NOT NULL CHECK (length IN (1, 2)),
    language text NOT NULL CHECK (char_length(language) BETWEEN 2 AND 16),
    pdf bytea NOT NULL,
    size_bytes integer NOT NULL CHECK (size_bytes > 0),
    pages smallint NOT NULL CHECK (pages BETWEEN 1 AND 2),
    -- The read-back: headings, keywords, lines, text layer, name, email, links.
    checks jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    -- The last time it was downloaded (Download, not View).
    downloaded_at timestamptz,
    UNIQUE (opportunity_id, version)
);

CREATE INDEX IF NOT EXISTS cv_versions_account_idx
    ON pipeline.cv_versions (account_id, created_at DESC);
CREATE INDEX IF NOT EXISTS cv_versions_draft_idx ON pipeline.cv_versions (draft_id);
