-- Match reasons are grounded in the profile (#27). Opportunities found
-- before the check have their reasons checked once, at start: the ones the
-- profile never supported are stripped (the opportunity stays, never hidden
-- or deleted) and the peek says "Reasons updated".
ALTER TABLE pipeline.opportunities
    -- The reasons were checked against the profile (new rows come checked).
    ADD COLUMN reasons_checked boolean NOT NULL DEFAULT false,
    -- The check stripped some reasons.
    ADD COLUMN reasons_updated boolean NOT NULL DEFAULT false;
ALTER TABLE pipeline.opportunities ALTER COLUMN reasons_checked SET DEFAULT true;

-- The one-off looks for Found and Shortlisted ones not yet checked.
CREATE INDEX opportunities_reasons_unchecked_idx ON pipeline.opportunities (id)
    WHERE NOT reasons_checked AND stage IN ('found', 'shortlisted');
