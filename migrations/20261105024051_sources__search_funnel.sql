-- A search reads more and drops less by rule (#74): every job within the
-- hard limits gets a quick look (title, company, place and the start of its
-- text, 25 a model call), and the best of them are read in full. A score is
-- kept per stage: `quick` (the quick look's fit and one reason) or `full`
-- (the full read, with the posting's facts). Rows from before are full reads.
-- Every statement can run twice.
ALTER TABLE sources.scores
    ADD COLUMN IF NOT EXISTS stage text NOT NULL DEFAULT 'full'
        CHECK (stage IN ('quick', 'full'));

ALTER TABLE sources.scores DROP CONSTRAINT IF EXISTS scores_pkey;
ALTER TABLE sources.scores
    ADD CONSTRAINT scores_pkey
        PRIMARY KEY (account_id, source, source_ref, profile_version, goals_version,
                     rules_version, stage);

-- What each search did with the jobs it read, for the Searches tab. NULL for
-- a search from before these were counted.
ALTER TABLE sources.searches
    -- Jobs given a quick look by this search.
    ADD COLUMN IF NOT EXISTS quick_looked integer CHECK (quick_looked >= 0),
    -- Jobs read in full by this search.
    ADD COLUMN IF NOT EXISTS read_in_full integer CHECK (read_in_full >= 0),
    -- Of those, the good matches kept although their posting does not say
    -- whether it hires where the person lives: they carry a note.
    ADD COLUMN IF NOT EXISTS kept_with_gap integer CHECK (kept_with_gap >= 0),
    -- Why every other job read was left out, by reason: their counts and the
    -- matches add up to the jobs read.
    ADD COLUMN IF NOT EXISTS dropped_by_reason jsonb;

-- Per board: jobs this search gave a quick look.
ALTER TABLE sources.board_scans
    ADD COLUMN IF NOT EXISTS quick_looked integer NOT NULL DEFAULT 0
        CHECK (quick_looked >= 0);
