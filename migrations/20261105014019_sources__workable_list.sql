-- The Workable list (#88): every company Workable publishes jobs for, read once a day.
-- The list names a company and one job's short code, never the company's address on
-- Workable (apply.workable.com/<token>). It is kept per company, with only how many jobs
-- it had; the address is found from one job's short link (it redirects to the company's
-- own page), a few hundred a day, and the company then becomes a company page.

CREATE TABLE sources.workable_list (
    -- The company's name in lower case: one row per company the list names.
    key text PRIMARY KEY CHECK (char_length(key) BETWEEN 1 AND 300),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 300),
    -- Its own site, as the list gives it (to apply the exclusion list).
    website text CHECK (char_length(website) <= 500),
    -- The short code of its newest job: the link apply.workable.com/j/<code> redirects to
    -- the company's page.
    sample_code text NOT NULL CHECK (sample_code ~ '^[A-Za-z0-9]{1,40}$'),
    -- Jobs the list had for it in the run that saw it last, and how many were posted in
    -- the 60 days before that run. Only these counts are kept, never the jobs.
    jobs integer NOT NULL DEFAULT 0,
    recent_jobs integer NOT NULL DEFAULT 0,
    newest_job_at timestamptz,
    last_seen_at timestamptz NOT NULL,
    run uuid NOT NULL,
    -- Its token on Workable, once found.
    token text CHECK (char_length(token) BETWEEN 1 AND 100),
    lookup_after timestamptz,
    lookup_error text CHECK (char_length(lookup_error) <= 100),
    -- When it was last written to the registry.
    promoted_at timestamptz
);
CREATE INDEX workable_list_lookup_idx ON sources.workable_list (newest_job_at DESC)
    WHERE token IS NULL AND recent_jobs > 0;

-- What the last download was, for the next one: one row.
CREATE TABLE sources.workable_list_state (
    id boolean PRIMARY KEY DEFAULT true CHECK (id),
    etag text,
    last_modified text,
    last_run_at timestamptz,
    -- No download before this (a 429's Retry-After).
    not_before timestamptz,
    -- complete, unchanged, interrupted, capped, refused, rate_limited, failed, skipped.
    outcome text,
    bytes bigint NOT NULL DEFAULT 0,
    jobs bigint NOT NULL DEFAULT 0,
    companies integer NOT NULL DEFAULT 0,
    added integer NOT NULL DEFAULT 0
);

-- A company page the list proves hiring: when the list last had it, and how many jobs.
ALTER TABLE sources.job_boards
    ADD COLUMN list_seen_at timestamptz,
    ADD COLUMN list_jobs integer;
