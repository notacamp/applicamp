-- The web backend-for-frontend's session store. The `bff` schema belongs to
-- `applicamp-server`, not to a module; module migrations may not touch it
-- (`cargo xtask arch`).
--
-- The session cookie holds a random 256-bit token; only its SHA-256 is
-- stored. Sessions last 30 days from their last use (sliding expiry).
CREATE SCHEMA IF NOT EXISTS bff;

CREATE TABLE bff.sessions (
    id uuid PRIMARY KEY,
    -- SHA-256 of the session cookie's token.
    token_hash bytea NOT NULL UNIQUE,
    -- The signed-in account (`identity.accounts.id`), by id: no foreign key
    -- across schemas.
    account_id uuid NOT NULL,
    created_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX sessions_account_idx ON bff.sessions (account_id);
-- Pruning finds ended sessions.
CREATE INDEX sessions_expires_at_idx ON bff.sessions (expires_at);
