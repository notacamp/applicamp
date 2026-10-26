-- The AI gateway's usage ledger (`applicamp-ai`). The `ai` schema belongs to
-- the gateway crate, not to a module; module migrations may not touch it
-- (`cargo xtask arch`).
--
-- One row per model call: whose it was, the role and model, and its tokens.
-- Ids and counts only, never a prompt or an answer. The daily cap
-- (`APPLICAMP_AI__DAILY_TOKENS`) sums an account's rows since UTC midnight.
CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.usage (
    id uuid PRIMARY KEY,
    -- The account (`identity.accounts.id`), by id: no foreign key across
    -- schemas.
    account_id uuid NOT NULL,
    role text NOT NULL CHECK (role IN ('complete')),
    model text NOT NULL,
    tokens_in integer NOT NULL CHECK (tokens_in >= 0),
    tokens_out integer NOT NULL CHECK (tokens_out >= 0),
    created_at timestamptz NOT NULL
);

CREATE INDEX usage_account_created_at_idx ON ai.usage (account_id, created_at);
