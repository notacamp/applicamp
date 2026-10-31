-- Tailored CVs, part A (#37): a draft per opportunity, built from the profile
-- with every line citing its sources, fact-checked without a model, reviewed
-- by the person and approved as a version. Nothing here is made in the
-- background: every row starts with a click.

CREATE TABLE IF NOT EXISTS pipeline.cv_drafts (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    -- Its number for the opportunity, from 1; never reused.
    n integer NOT NULL CHECK (n >= 1),
    -- A language tag (`en`, `pt-PT`).
    language text NOT NULL CHECK (char_length(language) BETWEEN 2 AND 16),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved', 'discarded')),
    -- Once approved: the version for the opportunity, from 1.
    version integer CHECK (version >= 1),
    -- The CV: contact, headline, summary, positions with bullets (each line
    -- cites source ids), skills, education, languages.
    content jsonb NOT NULL,
    -- What the lines may cite: the profile cut into sources, and the company
    -- facts, as they were when the draft was made (plus the facts added since).
    frame jsonb NOT NULL,
    -- The lines that cannot be traced to the profile, with why. Empty once
    -- approved.
    checks jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    approved_at timestamptz,
    -- 7 days after the last change; none once approved (a version is kept).
    expires_at timestamptz,
    UNIQUE (opportunity_id, n)
);

CREATE INDEX IF NOT EXISTS cv_drafts_account_day_idx ON pipeline.cv_drafts (account_id, created_at);
