-- The `identity` module: accounts, their ways in (Campbooks, LinkedIn), and
-- the invite list.
CREATE SCHEMA IF NOT EXISTS identity;

-- A person. `email` is the verified address the account was created with,
-- lower-cased.
CREATE TABLE identity.accounts (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE CHECK (email = lower(email)),
    email_verified boolean NOT NULL,
    name text,
    picture_url text,
    created_at timestamptz NOT NULL,
    last_sign_in_at timestamptz
);

-- A provider account that signs a person in. One account can have both.
CREATE TABLE identity.ways_in (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES identity.accounts (id) ON DELETE CASCADE,
    provider text NOT NULL CHECK (provider IN ('campbooks', 'linkedin')),
    -- The provider's `sub`.
    subject text NOT NULL,
    -- Campbooks' refresh token (for later API use), AES-256-GCM sealed under
    -- a key derived from the instance secret and bound to this row; never
    -- sent to the browser.
    refresh_token bytea,
    created_at timestamptz NOT NULL,
    last_used_at timestamptz NOT NULL,
    UNIQUE (provider, subject)
);

CREATE INDEX ways_in_account_idx ON identity.ways_in (account_id);

-- Who may create an account. Managed with `applicamp invite add|remove|list`
-- only; no HTTP endpoint changes it.
CREATE TABLE identity.invites (
    email text PRIMARY KEY CHECK (email = lower(email)),
    invited_at timestamptz NOT NULL,
    note text
);
