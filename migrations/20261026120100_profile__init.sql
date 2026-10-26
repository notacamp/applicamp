-- The `profile` module: the master profile, and the drafts imports make.
CREATE SCHEMA IF NOT EXISTS profile;

-- One profile per account. `data` is `crates/modules/profile/src/schema.rs`'s
-- `Profile` as JSON (JSON Resume's names). `source` is where it came from
-- last: the kind of the last import applied, or `manual`.
CREATE TABLE profile.profiles (
    -- The account (`identity.accounts.id`), by id: no foreign key across
    -- schemas.
    account_id uuid PRIMARY KEY,
    data jsonb NOT NULL,
    source text NOT NULL CHECK (source IN ('linkedin_zip', 'pdf', 'text', 'manual')),
    updated_at timestamptz NOT NULL
);

-- A draft waiting for review. Only the structured draft is kept, never the
-- uploaded file. Drafts live 24 hours; `profile.expire_imports` deletes them.
CREATE TABLE profile.imports (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('linkedin_zip', 'pdf', 'text')),
    status text NOT NULL CHECK (status IN ('ready', 'failed', 'applied', 'discarded')),
    draft jsonb,
    -- Values left out of the draft (the source check, the limits).
    dropped integer NOT NULL DEFAULT 0 CHECK (dropped >= 0),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX imports_account_idx ON profile.imports (account_id, created_at);
-- The expiry job deletes by it.
CREATE INDEX imports_expires_at_idx ON profile.imports (expires_at);
