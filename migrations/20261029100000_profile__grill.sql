-- Grill me (#33): a deep interview about one position at a time. A session
-- holds what has been covered; its turns are the questions, the person's
-- answers and short notes; a proposal is a fact taken from an answer that
-- waits for Keep, Fix or Drop. Kept facts are not stored here: they live in
-- the profile document, on the position (`work[].facts`), so every check and
-- every CV treats them as the person's own words.

-- One session per account and position (its index in `work`). `role_key` says
-- which position it was started for, so a list edited since never lends one
-- position's interview to another.
CREATE TABLE IF NOT EXISTS profile.grill_sessions (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    role_key text NOT NULL,
    -- area -> 'part' | 'ok'; an area missing is not covered. Only goes up.
    coverage jsonb NOT NULL DEFAULT '{}',
    -- Areas the person skipped since they last started: not asked again until
    -- they start again.
    skipped jsonb NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (account_id, position)
);

CREATE TABLE IF NOT EXISTS profile.grill_turns (
    session_id uuid NOT NULL REFERENCES profile.grill_sessions (id) ON DELETE CASCADE,
    seq integer NOT NULL CHECK (seq > 0),
    account_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('question', 'answer', 'note')),
    text text NOT NULL,
    -- A question that pushes back on a vague answer or asks about a contradiction.
    push boolean NOT NULL DEFAULT false,
    area text CHECK (area IS NULL OR area IN
        ('context', 'your_part', 'stack_scale', 'results', 'hard_problems', 'people', 'stories')),
    -- On an answer: lines the checks left out.
    dropped integer NOT NULL DEFAULT 0 CHECK (dropped >= 0),
    -- This turn is where a model call is counted against the daily cap.
    ai_call boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (session_id, seq)
);

CREATE INDEX IF NOT EXISTS grill_turns_calls_idx
    ON profile.grill_turns (account_id, created_at) WHERE ai_call;

-- A fact taken from an answer, waiting to be kept, fixed or dropped. They
-- live 24 hours, like drafts.
CREATE TABLE IF NOT EXISTS profile.grill_proposals (
    id uuid PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES profile.grill_sessions (id) ON DELETE CASCADE,
    account_id uuid NOT NULL,
    position integer NOT NULL CHECK (position >= 0),
    -- The answer it was taken from.
    turn_seq integer NOT NULL,
    text text NOT NULL,
    quote text NOT NULL,
    area text NOT NULL CHECK (area IN
        ('context', 'your_part', 'stack_scale', 'results', 'hard_problems', 'people', 'stories')),
    status text NOT NULL CHECK (status IN ('ready', 'kept', 'dropped')),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS grill_proposals_session_idx ON profile.grill_proposals (session_id);
CREATE INDEX IF NOT EXISTS grill_proposals_expires_at_idx ON profile.grill_proposals (expires_at);
