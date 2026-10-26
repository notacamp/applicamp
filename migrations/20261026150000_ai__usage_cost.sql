-- Spending limits in money (pipeline-and-search, #5): every model call's cost
-- in US dollars, as the provider reports it (OpenRouter's `usage.cost`) or as
-- the gateway's price table computes it.
--
-- Before a call the gateway reserves its worst case as a row (`settled` =
-- false) and checks the account's day and month and the instance's month
-- against their limits; once answered, the row is settled at the real cost.
-- Ids, amounts and counts only.
ALTER TABLE ai.usage
    ADD COLUMN cost_usd double precision NOT NULL DEFAULT 0 CHECK (cost_usd >= 0),
    ADD COLUMN settled boolean NOT NULL DEFAULT true;

-- The instance's monthly total sums every account's rows.
CREATE INDEX usage_created_at_idx ON ai.usage (created_at);
