-- Tell me about you (#43): what Applicamp knows and assumes about a person,
-- confirmed by them. One pending state per account, until they save it (then
-- it is gone) or seven days pass without a touch (`profile.expire_imports`
-- deletes it, like drafts). Nothing here is the profile: the profile is
-- written once, atomically, on Save.

CREATE TABLE IF NOT EXISTS profile.about_you (
    account_id uuid PRIMARY KEY,
    -- The profile version the portrait was built for; null while there is no
    -- portrait yet. A profile that changed since makes the portrait stale.
    profile_version integer,
    -- The portrait's sentences (known, told, assumed), as judged.
    portrait jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- What researching each of the latest employers came to.
    employers jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- Sentence id -> 'right' | 'wrong' | 'later' for the assumptions.
    choices jsonb NOT NULL DEFAULT '{}'::jsonb,
    accuracy text CHECK (accuracy IS NULL OR accuracy IN ('yes', 'some', 'no')),
    -- What the person wrote under "Anything else" (4,000 characters at most),
    -- the evidence for the changes below.
    extra text CHECK (extra IS NULL OR char_length(extra) <= 4000),
    -- The pending changes to the profile, with their flags.
    changes jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS about_you_expires_at_idx ON profile.about_you (expires_at);

-- Every model call the wizard made (a portrait, an "Anything else", an
-- employer's site read), for the daily cap.
CREATE TABLE IF NOT EXISTS profile.about_calls (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('portrait', 'extra', 'research')),
    created_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS about_calls_day_idx ON profile.about_calls (account_id, created_at);
