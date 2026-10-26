-- The sources module (#5): the job boards searched, each search's progress
-- and cost, the boards' answers (cached), and the scores the model gave.
CREATE SCHEMA IF NOT EXISTS sources;

-- The boards searched: the seed list (`crates/modules/sources/boards.toml`)
-- and the ones people added by pasting a posting's link.
CREATE TABLE sources.boards (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('greenhouse', 'lever', 'ashby')),
    token text NOT NULL,
    company text NOT NULL,
    -- A Lever board on Lever's EU instance.
    eu boolean NOT NULL DEFAULT false,
    -- `seed`, or the account (`identity.accounts.id`) that added it, by id.
    added_by text NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (kind, token)
);

-- One search: who for, what started it, how far it got, what it cost.
CREATE TABLE sources.searches (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    trigger text NOT NULL CHECK (trigger IN ('manual', 'nightly')),
    status text NOT NULL CHECK (status IN ('queued', 'running', 'finished', 'failed')),
    started_at timestamptz NOT NULL,
    finished_at timestamptz,
    -- Jobs read, kept by the hard limits, scored, and added as Found.
    fetched integer NOT NULL DEFAULT 0,
    kept integer NOT NULL DEFAULT 0,
    scored integer NOT NULL DEFAULT 0,
    added integer NOT NULL DEFAULT 0,
    -- Model calls made, and what they cost in US dollars.
    batches integer NOT NULL DEFAULT 0,
    cost_usd double precision NOT NULL DEFAULT 0,
    -- Why it stopped early: ai_budget_reached, ai_unavailable, ai_failed,
    -- no_profile, sources_failed.
    error_kind text
);

CREATE INDEX searches_account_started_idx ON sources.searches (account_id, started_at DESC);

-- The boards' answers, reused for a while (6 hours by default) so a board
-- is not asked twice for the same thing.
CREATE TABLE sources.cache (
    url text PRIMARY KEY,
    fetched_at timestamptz NOT NULL,
    body bytea NOT NULL
);

-- The model's score of a job for an account, for one version of their
-- profile and goals: the same job is never scored twice for them.
CREATE TABLE sources.scores (
    account_id uuid NOT NULL,
    source text NOT NULL,
    source_ref text NOT NULL,
    profile_version text NOT NULL,
    goals_version integer NOT NULL,
    -- 0-100; 0 also for an HN comment the model could not read.
    match_score integer NOT NULL CHECK (match_score BETWEEN 0 AND 100),
    why jsonb NOT NULL DEFAULT '[]'::jsonb,
    gaps jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- What the model read from an unstructured posting (HN): company,
    -- title, location, work mode, URL, pay.
    read jsonb,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (account_id, source, source_ref, profile_version, goals_version)
);
