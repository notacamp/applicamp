-- My sites (#94): which job sites each person has On. The app sets On or Off
-- from the person's goals and the sites' tags (`set_by` = 'app', set again
-- whenever the goals change); a person's own choice (`set_by` = 'person') is
-- never overwritten. Searches read only a person's On sites (#89).
CREATE TABLE sources.my_sites (
    -- `identity.accounts.id`, by id: no foreign key across schemas.
    account_id uuid NOT NULL,
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    is_on boolean NOT NULL,
    set_by text NOT NULL CHECK (set_by IN ('app', 'person')),
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (account_id, board_id)
);
CREATE INDEX my_sites_board_idx ON sources.my_sites (board_id);
