-- Job boards (#5): the registry of boards a search may read (approved by the
-- owner), the candidates discovery found and checked, the jobs each board
-- listed lately (for its average salary), discovery's runs, and what every
-- search and discovery check scanned, board by board.

-- One job board: how to read it (config), what its terms ask (terms), what
-- the automatic check found (report), and whether the owner approved it.
CREATE TABLE sources.job_boards (
    id uuid PRIMARY KEY,
    -- Its stable name, which jobs and opportunities carry as their source.
    slug text NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    homepage text NOT NULL CHECK (char_length(homepage) <= 500),
    -- The homepage's host, lower case, without `www.`: one board per site.
    host text NOT NULL UNIQUE,
    kind text NOT NULL CHECK (kind IN ('json_api', 'rss', 'jsonld_sitemap', 'hn')),
    -- high: remote-first, open worldwide; low: national, regional or general.
    priority text NOT NULL CHECK (priority IN ('high', 'low')),
    status text NOT NULL CHECK (status IN ('candidate', 'approved', 'paused', 'declined')),
    origin text NOT NULL CHECK (origin IN ('seed', 'discovery', 'suggested')),
    -- The one country its jobs are in, when it is country-scoped (ISO 3166-1).
    country text CHECK (country ~ '^[A-Z]{2}$'),
    config jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- The owner edited the config: the seed list no longer overwrites it.
    config_edited boolean NOT NULL DEFAULT false,
    terms jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- The automatic check (robots, terms sentences, feed, sample jobs).
    report jsonb,
    decline_reason text CHECK (char_length(decline_reason) <= 500),
    -- Who suggested it, approved it (`identity.accounts.id`, by id).
    suggested_by uuid,
    approved_at timestamptz,
    approved_by uuid,
    -- When its robots rules and terms are checked again (every 6 months).
    recheck_after timestamptz,
    last_fetch_at timestamptz,
    last_fetch_jobs integer,
    last_error text,
    -- It asked us to slow down (429/503 with Retry-After): not read before.
    backoff_until timestamptz,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

-- The jobs a board listed lately (30 days), as much as its average salary
-- needs: never shown, never searched. `sample` rows are a candidate's check.
CREATE TABLE sources.board_jobs (
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    source_ref text NOT NULL,
    title text NOT NULL,
    -- Who may apply, as the board's structured field says.
    regions text[] NOT NULL DEFAULT '{}',
    -- An engineering role (by its title).
    engineering boolean NOT NULL,
    -- The stated pay's midpoint, in euros a year; NULL when none or not
    -- convertible.
    salary_eur_year double precision,
    posted_at timestamptz,
    seen_at timestamptz NOT NULL,
    sample boolean NOT NULL DEFAULT false,
    PRIMARY KEY (board_id, source_ref)
);

-- A weekly discovery run: candidates found, checked, boards re-checked.
CREATE TABLE sources.discovery_runs (
    id uuid PRIMARY KEY,
    started_at timestamptz NOT NULL,
    finished_at timestamptz,
    found integer NOT NULL DEFAULT 0,
    checked integer NOT NULL DEFAULT 0,
    rechecked integer NOT NULL DEFAULT 0,
    error_kind text
);

-- What one search (or one discovery run) scanned on one board: jobs read,
-- passed the person's requirements, scored by the AI, matched; what the AI
-- cost; why it was skipped or failed. Kept 90 days.
CREATE TABLE sources.board_scans (
    id uuid PRIMARY KEY,
    -- Whose search (`identity.accounts.id`, by id); NULL for discovery.
    account_id uuid,
    search_id uuid REFERENCES sources.searches (id) ON DELETE CASCADE,
    discovery_run_id uuid REFERENCES sources.discovery_runs (id) ON DELETE CASCADE,
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    at timestamptz NOT NULL,
    outcome text NOT NULL CHECK (outcome IN ('read', 'skipped_poll_limit', 'error', 'checked')),
    jobs_read integer NOT NULL DEFAULT 0,
    passed_requirements integer NOT NULL DEFAULT 0,
    scored_by_ai integer NOT NULL DEFAULT 0,
    matched integer NOT NULL DEFAULT 0,
    ai_cost_eur double precision NOT NULL DEFAULT 0,
    error_kind text,
    duration_ms integer NOT NULL DEFAULT 0,
    -- What a discovery check found, in a few words.
    note text CHECK (char_length(note) <= 300),
    CHECK ((search_id IS NULL) <> (discovery_run_id IS NULL))
);

CREATE INDEX board_scans_account_at_idx ON sources.board_scans (account_id, at DESC);
CREATE INDEX board_scans_at_idx ON sources.board_scans (at);
CREATE INDEX board_scans_board_at_idx ON sources.board_scans (board_id, at DESC);
