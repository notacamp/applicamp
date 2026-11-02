-- Cover letters (#45): the CV's second document, for the opportunities whose
-- CV is approved. A draft is paragraphs of sentences that each cite sources,
-- plus answers to the job's own questions; Approve renders it as a one-page
-- PDF kept as a version. Nothing here is made in the background: every row
-- starts with a click. Letter drafts have their own table (and so their own
-- caps): a letter's content is not a CV's.

CREATE TABLE IF NOT EXISTS pipeline.letter_drafts (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    -- Its number for the opportunity, from 1; never reused.
    n integer NOT NULL CHECK (n >= 1),
    -- A language tag (`en`, `pt-PT`).
    language text NOT NULL CHECK (char_length(language) BETWEEN 2 AND 16),
    -- About 180 words or about 300.
    size text NOT NULL DEFAULT 'short' CHECK (size IN ('short', 'standard')),
    -- The version of the approved CV it was written next to.
    cv_version integer CHECK (cv_version >= 1),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved', 'discarded')),
    -- Once approved: the version for the opportunity, from 1.
    version integer CHECK (version >= 1),
    -- The letter: contact, paragraphs of sentences (each cites source ids),
    -- and the answers to the job's questions.
    content jsonb NOT NULL,
    -- What the sentences may cite: the profile cut into sources, and the
    -- company facts, as they were when the draft was made (plus the facts
    -- added since).
    frame jsonb NOT NULL,
    -- The sentences and answers that cannot be traced, with why. Empty once
    -- approved.
    checks jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    approved_at timestamptz,
    -- 7 days after the last change; none once approved (a version is kept).
    expires_at timestamptz,
    UNIQUE (opportunity_id, n)
);

CREATE INDEX IF NOT EXISTS letter_drafts_account_day_idx
    ON pipeline.letter_drafts (account_id, created_at);

-- What was approved, as a PDF read back and found to say what the letter says.
-- Never overwritten. Stored here, with the account's data and its backups.
CREATE TABLE IF NOT EXISTS pipeline.letter_versions (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    draft_id uuid NOT NULL REFERENCES pipeline.letter_drafts (id) ON DELETE CASCADE,
    version integer NOT NULL CHECK (version >= 1),
    language text NOT NULL CHECK (char_length(language) BETWEEN 2 AND 16),
    pdf bytea NOT NULL,
    size_bytes integer NOT NULL CHECK (size_bytes > 0),
    pages smallint NOT NULL CHECK (pages = 1),
    checks jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    downloaded_at timestamptz,
    UNIQUE (opportunity_id, version)
);

CREATE INDEX IF NOT EXISTS letter_versions_account_idx
    ON pipeline.letter_versions (account_id, created_at DESC);
CREATE INDEX IF NOT EXISTS letter_versions_draft_idx ON pipeline.letter_versions (draft_id);
