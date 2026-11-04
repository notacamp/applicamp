-- What a CV draft cost (#73): the draft, the second look at what the job
-- wants and the app's own fix of what the check flagged, in euros, as the
-- provider billed them. Shown as "Done by the app".

ALTER TABLE pipeline.cv_drafts
    ADD COLUMN IF NOT EXISTS cost_eur double precision CHECK (cost_eur IS NULL OR cost_eur >= 0);
