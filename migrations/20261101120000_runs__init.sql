-- The runs module (#41, #5, #6): agent runs in throwaway sandboxes, what
-- they did, and the single-run tokens their agents call the server with.
-- Ids, kinds, counts and the agents' own event lines only.
CREATE SCHEMA IF NOT EXISTS runs;

-- The run queue and each run's facts. A run is claimed from here by the
-- orchestrator loop (`FOR UPDATE SKIP LOCKED`), whose claim is renewed while
-- the run goes; a claim that lapses means its server died.
CREATE TABLE runs.runs (
    id uuid PRIMARY KEY,
    -- The account (`identity.accounts.id`), by id: no foreign key across
    -- schemas.
    account_id uuid NOT NULL,
    -- `<module>.<kind>`.
    kind text NOT NULL,
    -- What it is about (an opportunity, a company), by id.
    subject_id uuid,
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'starting', 'running', 'finished')),
    -- How it ended; while it is still running, what the agent finished with.
    outcome text CHECK (outcome IN ('succeeded', 'nothing_new', 'needs_you', 'ended_max_steps',
                                    'ended_budget', 'ended_timeout', 'ended_loop', 'failed',
                                    'cancelled')),
    inputs jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Set when it starts: the start plus its kind's wall clock.
    deadline timestamptz,
    created_at timestamptz NOT NULL,
    started_at timestamptz,
    finished_at timestamptz,
    -- The sandbox's name at its runtime, its image and policy, and the model
    -- that answered its last completion.
    sandbox text,
    image text,
    policy text,
    model text,
    tokens_in integer NOT NULL DEFAULT 0 CHECK (tokens_in >= 0),
    tokens_out integer NOT NULL DEFAULT 0 CHECK (tokens_out >= 0),
    cost_eur double precision NOT NULL DEFAULT 0 CHECK (cost_eur >= 0),
    -- Why it failed, as a code (`sandbox_create_failed`, `server_restarted`).
    error_kind text,
    -- What the agent finished with, checked against its kind's schema.
    summary jsonb,
    cancel_requested boolean NOT NULL DEFAULT false,
    -- The server running it, and until when its claim holds.
    claimed_by text,
    claimed_until timestamptz,
    -- The last event number given out (`runs.events.seq`).
    last_seq bigint NOT NULL DEFAULT 0,
    CHECK ((status = 'finished') = (finished_at IS NOT NULL))
);

CREATE INDEX runs_queue_idx ON runs.runs (created_at) WHERE status = 'queued';
CREATE INDEX runs_live_idx ON runs.runs (account_id) WHERE status IN ('starting', 'running');
CREATE INDEX runs_account_created_idx ON runs.runs (account_id, created_at DESC);

-- What a run did: the agent's own lines (layer `agent`) and the sandbox's
-- allow and deny lines (layer `sandbox`). `seq` is the live stream's
-- `Last-Event-ID`; `agent_seq` is the agent's own number, so a line read
-- from its output and also sent through the agent API is stored once.
-- Kept 30 days.
CREATE TABLE runs.events (
    run_id uuid NOT NULL REFERENCES runs.runs (id) ON DELETE CASCADE,
    seq bigint NOT NULL,
    agent_seq bigint,
    at timestamptz NOT NULL,
    layer text NOT NULL CHECK (layer IN ('agent', 'sandbox')),
    level text NOT NULL CHECK (level IN ('info', 'tool', 'model', 'deny', 'ask', 'error')),
    operation text CHECK (char_length(operation) <= 100),
    tool_name text CHECK (char_length(tool_name) <= 100),
    message text NOT NULL CHECK (char_length(message) <= 2000),
    data jsonb,
    PRIMARY KEY (run_id, seq),
    UNIQUE (run_id, agent_seq)
);

CREATE INDEX events_at_idx ON runs.events (at);

-- A run's token: 32 random bytes sent once into its sandbox, kept as their
-- SHA-256, with the tools of its kind, their caps and the calls made so far.
-- Deleted when the run finishes; never valid past its deadline + 5 minutes.
CREATE TABLE runs.tokens (
    run_id uuid PRIMARY KEY REFERENCES runs.runs (id) ON DELETE CASCADE,
    hash bytea NOT NULL UNIQUE,
    -- `{ "<tool>": <cap> }`.
    tools jsonb NOT NULL,
    -- `{ "<tool>": <calls> }`.
    calls jsonb NOT NULL DEFAULT '{}'::jsonb,
    expires_at timestamptz NOT NULL
);
