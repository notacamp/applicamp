-- A search reads the job store (#89, second part). What it adds to the
-- Searches tab: how many sites and company pages the store gave it jobs
-- from, and what its quick looks and full reads were expected to cost.
-- NULL for a search from before. Every statement can run twice.
ALTER TABLE sources.searches
    -- Job sites the store gave this search jobs from.
    ADD COLUMN IF NOT EXISTS from_sites integer CHECK (from_sites >= 0),
    -- Company pages the store gave this search jobs from.
    ADD COLUMN IF NOT EXISTS from_company_pages integer CHECK (from_company_pages >= 0),
    -- What the search expected its model calls to cost (US dollars), worked
    -- out before the first one from the jobs it chose to look at.
    ADD COLUMN IF NOT EXISTS expected_cost_usd double precision CHECK (expected_cost_usd >= 0);
