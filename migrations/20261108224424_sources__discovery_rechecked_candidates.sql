-- Candidates checked again under new rules (#113), counted apart from the
-- approved boards re-checked (`rechecked`).
ALTER TABLE sources.discovery_runs
    ADD COLUMN rechecked_candidates integer NOT NULL DEFAULT 0;
