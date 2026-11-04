-- Your LinkedIn (#84): a review of the LinkedIn profile the person uploaded,
-- against their profile and goals. One per account: a new check-up replaces
-- it, Start over deletes it, and `profile.expire_imports` deletes it 30 days
-- after its last touch. Nothing here is the profile: a check-up only reads it.

CREATE TABLE IF NOT EXISTS profile.linkedin_reviews (
    account_id uuid PRIMARY KEY,
    -- What the LinkedIn profile was read from.
    kind text NOT NULL CHECK (kind IN ('linkedin_zip', 'pdf')),
    language text NOT NULL,
    -- What the review was made against; a profile that changed since makes
    -- the review stale (said, never blocking).
    profile_version integer NOT NULL,
    goals_version integer NOT NULL,
    -- The LinkedIn profile as read, without the person's name, email,
    -- location or links, and with contact details scrubbed from its texts.
    linkedin jsonb NOT NULL,
    -- How the LinkedIn profile reads to a recruiter for the wanted roles.
    overall text NOT NULL,
    -- One finding per part (headline, About, each position, skills…).
    findings jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- The proposals, each with its sources, flags, status and text.
    proposals jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS linkedin_reviews_expires_at_idx ON profile.linkedin_reviews (expires_at);

-- Every check-up made, for the daily cap.
CREATE TABLE IF NOT EXISTS profile.linkedin_calls (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS linkedin_calls_day_idx ON profile.linkedin_calls (account_id, created_at);
