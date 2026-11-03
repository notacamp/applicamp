-- Company watchers (#20): companies whose jobs keep fitting a person, whose own
-- job board is read once a day, and the postings already seen on it.
--
-- `companies` describes a company and its job board and belongs to nobody, so
-- a later run kind ("find companies like these") can add rows to it. What one
-- person does with a company (the suggestion, the watch, the diff against
-- yesterday) is `company_watches` and `watch_seen`.

CREATE TABLE sources.companies (
    id uuid PRIMARY KEY,
    -- `site:<host>` (the website's host without `www.`) or
    -- `ats:<kind>:<token>` for a company only known by its job board.
    key text NOT NULL UNIQUE CHECK (char_length(key) BETWEEN 1 AND 400),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 300),
    -- The company's own site, an origin.
    website text CHECK (char_length(website) <= 2000),
    -- The page that lists its jobs (found, or pasted).
    careers_url text CHECK (char_length(careers_url) <= 2000),
    -- How its jobs are read. `none`: no board found (yet).
    board_kind text NOT NULL DEFAULT 'none'
        CHECK (board_kind IN ('greenhouse', 'lever', 'ashby', 'jsonld', 'none')),
    board_token text CHECK (char_length(board_token) <= 100),
    -- A Lever board on Lever's EU instance.
    board_eu boolean NOT NULL DEFAULT false,
    -- When the board was last confirmed to read jobs.
    detected_at timestamptz,
    -- When a board was last looked for.
    checked_at timestamptz,
    -- How the company came here: `matches` (a person's matches), `pasted` (a
    -- careers page someone pasted), `agent` (a later run kind).
    origin text NOT NULL CHECK (origin IN ('matches', 'pasted', 'agent')),
    created_at timestamptz NOT NULL
);

CREATE TABLE sources.company_watches (
    id uuid PRIMARY KEY,
    -- The account (`identity.accounts.id`), by id.
    account_id uuid NOT NULL,
    company_id uuid NOT NULL REFERENCES sources.companies (id) ON DELETE CASCADE,
    state text NOT NULL CHECK (state IN ('suggested', 'watching', 'not_interested', 'stopped')),
    -- Why it is suggested: each reason cites the postings (or company facts)
    -- it comes from. Nothing here is written by a model.
    reasons jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    -- When the person said Watch.
    watching_since timestamptz,
    -- While a board is being looked for (cleared when it ends; one older than
    -- 10 minutes is taken as dead).
    finding_since timestamptz,
    -- When the daily check last read the board, and what it said.
    last_checked_at timestamptz,
    last_error text,
    -- Jobs open on the board at the last read.
    open_jobs integer,
    -- When the postings already there were noted as seen (the first read
    -- scores nothing: the watch is "from today").
    seeded_at timestamptz,
    UNIQUE (account_id, company_id)
);

CREATE INDEX company_watches_account_idx ON sources.company_watches (account_id, state);

-- The postings seen on a watched board, to diff against the next read.
CREATE TABLE sources.watch_seen (
    watch_id uuid NOT NULL REFERENCES sources.company_watches (id) ON DELETE CASCADE,
    source text NOT NULL,
    source_ref text NOT NULL,
    first_seen_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    PRIMARY KEY (watch_id, source, source_ref)
);

CREATE INDEX watch_seen_last_idx ON sources.watch_seen (last_seen_at);

-- When each account's suggestions were last worked out.
CREATE TABLE sources.watch_refreshes (
    account_id uuid PRIMARY KEY,
    at timestamptz NOT NULL
);

-- A watch check is a scan row like a job board's: it names a company instead.
ALTER TABLE sources.board_scans
    ALTER COLUMN board_id DROP NOT NULL,
    ADD COLUMN company_id uuid REFERENCES sources.companies (id) ON DELETE CASCADE,
    ADD CONSTRAINT board_scans_board_or_company CHECK ((board_id IS NULL) <> (company_id IS NULL));
