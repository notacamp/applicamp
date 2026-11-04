-- The `write` role (#73): longer writing (CVs, letters) on its own model.
-- Its calls are in the same ledger, under the same daily cap and spending
-- limits, with their role.

ALTER TABLE ai.usage DROP CONSTRAINT IF EXISTS usage_role_check;
ALTER TABLE ai.usage
    ADD CONSTRAINT usage_role_check CHECK (role IN ('complete', 'write'));
