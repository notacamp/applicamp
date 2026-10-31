-- A run's model tokens are reserved before a completion is forwarded (ADR
-- 0009: the server reserves each call's worst case first), so parallel calls
-- cannot together pass the kind's budget. Settled to what the call used once
-- it answers, released when it fails.
ALTER TABLE runs.runs
    ADD COLUMN tokens_reserved integer NOT NULL DEFAULT 0 CHECK (tokens_reserved >= 0);
