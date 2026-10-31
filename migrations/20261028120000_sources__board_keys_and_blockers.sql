-- Board approvals (#26): what forbids reading a board, as the research found
-- it (its terms, its robots rules: shown, and they block approval), and the
-- API key a board needs, sealed with the instance secret (AES-256-GCM, bound
-- to the board's id) and never sent to a client.
ALTER TABLE sources.job_boards
    -- `[{code, detail, source}]`: `terms_forbid` or `robots_disallowed`,
    -- refreshed from the seed list at every start.
    ADD COLUMN blockers jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- The key's parameters and values, sealed; NULL when none is set.
    ADD COLUMN api_key bytea,
    -- The host the key was set for: it is sent to no other.
    ADD COLUMN api_key_host text CHECK (char_length(api_key_host) <= 255),
    ADD COLUMN api_key_set_at timestamptz;
