-- Complete your profile (#72): the facts Applicamp already knows about the
-- person (what they told and confirmed in Tell me about you, what their own
-- website says, what a CV read again says), proposed for the profile, each to
-- keep or drop. Nothing here is the profile: keeping one writes it on its
-- position (`work[].facts`), the skills or the background, as a new profile
-- version. A decision can be undone for a day.

CREATE TABLE IF NOT EXISTS profile.fact_proposals (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    -- Where it goes: a position (by its place in `work` when proposed and its
    -- key, so a list edited since never lends it to another), the skills, or
    -- the background.
    target text NOT NULL CHECK (target IN ('position', 'skills', 'background')),
    position integer CHECK (position IS NULL OR position >= 0),
    role_key text,
    -- The fact, the words it comes from, and where those words come from.
    text text NOT NULL,
    quote text NOT NULL,
    source text NOT NULL CHECK (source IN ('you_said', 'you_confirmed', 'your_website', 'your_cv')),
    -- For a confirmed assumption: what it was based on (its reason, or a page).
    basis text,
    area text NOT NULL CHECK (area IN
        ('context', 'your_part', 'stack_scale', 'results', 'hard_problems', 'people', 'stories')),
    state text NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'kept', 'dropped')),
    -- For a kept fact: the fact's id on the position, so Undo removes it.
    fact_id text,
    created_at timestamptz NOT NULL,
    decided_at timestamptz,
    CHECK ((target = 'position') = (position IS NOT NULL AND role_key IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS fact_proposals_account_idx
    ON profile.fact_proposals (account_id, state, created_at);

-- One-off work done once per instance (the proposals for the accounts that
-- existed before this change): its name, and when it was done.
CREATE TABLE IF NOT EXISTS profile.markers (
    name text PRIMARY KEY,
    done_at timestamptz NOT NULL
);
