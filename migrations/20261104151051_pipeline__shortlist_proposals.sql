-- Shortlist proposals (#73, ADR 0010): a job that fits well (a match of 75 or
-- more, open to the person) is proposed to them; with the Autonomy goal
-- "Prepare", the best few are shortlisted by the app after a search.

ALTER TABLE pipeline.opportunities
    -- When it was proposed; none once its stage changes or it is set aside.
    ADD COLUMN IF NOT EXISTS proposed_at timestamptz,
    -- Who shortlisted it: the person, or the app (Autonomy "Prepare").
    ADD COLUMN IF NOT EXISTS shortlisted_by text
        CHECK (shortlisted_by IS NULL OR shortlisted_by IN ('person', 'app')),
    ADD COLUMN IF NOT EXISTS shortlisted_at timestamptz;

CREATE INDEX IF NOT EXISTS opportunities_proposed_idx
    ON pipeline.opportunities (account_id, match_score DESC)
    WHERE proposed_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS opportunities_shortlisted_by_app_idx
    ON pipeline.opportunities (account_id, shortlisted_at)
    WHERE shortlisted_by = 'app';
