-- Profile completion (#24): a version for the profile, the gap review the
-- model makes once per version, drafts made from answers, and skills the
-- person said are not theirs.

-- The profile's version: every change to its data adds one, so a review and a
-- draft can say which profile they were made from. A trigger keeps it, so no
-- write can forget.
ALTER TABLE profile.profiles ADD COLUMN IF NOT EXISTS version integer NOT NULL DEFAULT 1;

CREATE OR REPLACE FUNCTION profile.bump_profile_version() RETURNS trigger AS $$
BEGIN
    IF NEW.data IS DISTINCT FROM OLD.data THEN
        NEW.version := OLD.version + 1;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS profiles_bump_version ON profile.profiles;
CREATE TRIGGER profiles_bump_version
    BEFORE UPDATE ON profile.profiles
    FOR EACH ROW EXECUTE FUNCTION profile.bump_profile_version();

-- Where the profile came from last gains `answer`: a draft the person kept.
ALTER TABLE profile.profiles DROP CONSTRAINT IF EXISTS profiles_source_check;
ALTER TABLE profile.profiles ADD CONSTRAINT profiles_source_check
    CHECK (source IN ('linkedin_zip', 'pdf', 'text', 'manual', 'answer'));

-- The gap review: one per account, for one profile version and one goals
-- version. `questions` and `skills` are the validated model answer;
-- `resolved` maps a question id to `kept` or `skipped`. A review the
-- person's own kept answers made stale is carried forward to the new
-- profile version instead of being asked for again.
CREATE TABLE IF NOT EXISTS profile.gap_reviews (
    account_id uuid PRIMARY KEY,
    profile_version integer NOT NULL,
    goals_version integer NOT NULL,
    status text NOT NULL CHECK (status IN ('running', 'ready', 'failed')),
    -- The problem code of a failed review (`ai_budget_reached`, …).
    error text,
    questions jsonb NOT NULL DEFAULT '[]',
    skills jsonb NOT NULL DEFAULT '[]',
    resolved jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL
);

-- A draft made from an answer, waiting to be kept, edited or discarded. The
-- answer itself is not stored. Drafts live 24 hours.
CREATE TABLE IF NOT EXISTS profile.drafts (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    question_id text NOT NULL,
    -- The position a `highlights` draft belongs to (its index in `work`).
    role_ref integer,
    profile_version integer NOT NULL,
    kind text NOT NULL CHECK (kind IN ('highlights', 'label', 'summary', 'education', 'link')),
    items jsonb NOT NULL,
    education jsonb,
    dropped integer NOT NULL DEFAULT 0 CHECK (dropped >= 0),
    status text NOT NULL CHECK (status IN ('ready', 'kept', 'discarded')),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS drafts_account_idx ON profile.drafts (account_id, question_id);
CREATE INDEX IF NOT EXISTS drafts_expires_at_idx ON profile.drafts (expires_at);

-- Skills the person said are not theirs, by lower-case name: not suggested again.
CREATE TABLE IF NOT EXISTS profile.skill_dismissals (
    account_id uuid NOT NULL,
    name_key text NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (account_id, name_key)
);
