-- Hard requirements are pass/fail (#19): the score cache keeps the facts the
-- model read with each score, and why the rules left the job out, per version
-- of the rules (a change to what the model is asked re-asks it once). Rows of
-- version 0 are from before the facts: never read again.
ALTER TABLE sources.scores
    ADD COLUMN rules_version integer NOT NULL DEFAULT 0,
    ADD COLUMN remote text CHECK (remote IN ('yes', 'no', 'unclear')),
    ADD COLUMN open_to_candidate text CHECK (open_to_candidate IN ('yes', 'no', 'unclear')),
    ADD COLUMN region_note text,
    ADD COLUMN working_languages text[],
    -- The pay it states: {min, max, currency, period}.
    ADD COLUMN salary jsonb,
    -- not_remote, not_open_to_location, language, below_floor; NULL: it meets
    -- every requirement.
    ADD COLUMN excluded_reason text;

ALTER TABLE sources.scores DROP CONSTRAINT scores_pkey;
ALTER TABLE sources.scores
    ADD PRIMARY KEY (account_id, source, source_ref, profile_version, goals_version, rules_version);

-- Jobs a search scored that fail a requirement: never added.
ALTER TABLE sources.searches ADD COLUMN dropped integer NOT NULL DEFAULT 0;
