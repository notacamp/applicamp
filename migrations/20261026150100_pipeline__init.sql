-- The pipeline module (#5): every opportunity a person has, found by a search
-- or added by link, with its stage, match, notes and next step.
CREATE SCHEMA IF NOT EXISTS pipeline;

CREATE TABLE pipeline.opportunities (
    id uuid PRIMARY KEY,
    -- The account (`identity.accounts.id`), by id: no foreign key across
    -- schemas.
    account_id uuid NOT NULL,
    title text NOT NULL,
    company text NOT NULL,
    location text,
    work_mode text CHECK (work_mode IN ('remote', 'hybrid', 'onsite')),
    url text NOT NULL,
    source text NOT NULL
        CHECK (source IN ('greenhouse', 'lever', 'ashby', 'arbeitnow', 'hn', 'link')),
    -- The posting's id at its source (`<board>:<id>` on Greenhouse); a
    -- link-added one's is its URL.
    source_ref text NOT NULL,
    posted_at timestamptz,
    salary_text text,
    -- Plain text, at most 20 000 characters.
    description text CHECK (char_length(description) <= 20000),
    match_score integer CHECK (match_score BETWEEN 0 AND 100),
    -- Short reasons citing the profile, and gaps: JSON lists of strings.
    match_why jsonb NOT NULL DEFAULT '[]'::jsonb,
    match_gaps jsonb NOT NULL DEFAULT '[]'::jsonb,
    stage text NOT NULL DEFAULT 'found'
        CHECK (stage IN ('found', 'shortlisted', 'applying', 'applied', 'talking',
                         'interview', 'offer', 'closed')),
    next_step text,
    notes text,
    dismissed boolean NOT NULL DEFAULT false,
    -- When it was first opened; a Found one never opened is new.
    opened_at timestamptz,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (account_id, source, source_ref)
);

CREATE INDEX opportunities_account_stage_idx ON pipeline.opportunities (account_id, stage);
CREATE INDEX opportunities_account_created_idx ON pipeline.opportunities (account_id, created_at DESC);
