-- Goals: what a person wants next, answered one question at a time.
-- `answers` is `crates/modules/profile/src/goals.rs`'s `Goals` as JSON. Every
-- save adds one to `version`, so an agent can record which version it used.
CREATE TABLE profile.goals (
    -- The account (`identity.accounts.id`), by id: no foreign key across
    -- schemas.
    account_id uuid PRIMARY KEY,
    version integer NOT NULL CHECK (version >= 1),
    answers jsonb NOT NULL,
    updated_at timestamptz NOT NULL
);
