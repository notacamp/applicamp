-- Kernel infrastructure: the outbox (domain events) and the job queue. The
-- `kernel` schema belongs to `applicamp-kernel`, not to a module; module
-- migrations may not touch it (`cargo xtask arch`). Ids are UUIDv7 made by
-- the server.
CREATE SCHEMA IF NOT EXISTS kernel;

-- Every domain event ever published, append-only. Written in the
-- publisher's transaction together with the jobs that deliver it to each
-- consumer, so there is no event without its data change and no data change
-- without its event. Payloads hold ids, kinds and counts only.
CREATE TABLE kernel.events (
    id uuid PRIMARY KEY,
    -- `DomainEvent::NAME`, `<module>.<event>`.
    name text NOT NULL,
    -- `DomainEvent::VERSION`, the payload schema version.
    version integer NOT NULL CHECK (version > 0),
    payload jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX events_name_occurred_at_idx ON kernel.events (name, occurred_at);

-- The job queue. Workers claim rows with `FOR UPDATE SKIP LOCKED`.
CREATE TABLE kernel.jobs (
    id uuid PRIMARY KEY,
    -- `Job::NAME` or `EventHandler::NAME`.
    kind text NOT NULL,
    queue text NOT NULL,
    payload jsonb NOT NULL,
    -- For event deliveries: the event in `kernel.events` being delivered.
    event_id uuid REFERENCES kernel.events (id),
    state text NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'running', 'succeeded', 'failed')),
    run_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts integer NOT NULL CHECK (max_attempts > 0),
    -- Until when the worker running it holds it.
    locked_until timestamptz,
    -- The last failure, truncated; never carries secrets.
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    CHECK ((state = 'running') = (locked_until IS NOT NULL))
);

-- Claiming: ready work in order.
CREATE INDEX jobs_ready_idx ON kernel.jobs (run_at, id) WHERE state = 'pending';
-- Reclaiming expired leases.
CREATE INDEX jobs_lease_idx ON kernel.jobs (locked_until) WHERE state = 'running';
-- Pruning succeeded jobs; inspecting dead ones.
CREATE INDEX jobs_finished_idx ON kernel.jobs (state, finished_at)
    WHERE state IN ('succeeded', 'failed');
