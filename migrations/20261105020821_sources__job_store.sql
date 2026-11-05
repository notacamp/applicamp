-- The job store (#89): every approved site and company page is read on its
-- own schedule, and what it lists is kept here, normalised the way the
-- readers give it, so a search can read the store instead of the web.
-- No model is called while reading.

-- One row per job per registry row. A job seen again is touched
-- (`last_seen_at`); one the site no longer lists is closed (`closed_at`) and
-- deleted later; a job that changed is rewritten.
CREATE TABLE sources.store_jobs (
    id uuid PRIMARY KEY,
    -- The registry row it was read from: a job board, or a company page.
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    -- The job's id on that site (`<board>:<id>` on Greenhouse).
    source_ref text NOT NULL CHECK (char_length(source_ref) BETWEEN 1 AND 500),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 300),
    company text NOT NULL DEFAULT '' CHECK (char_length(company) <= 300),
    -- The posting on the site (what the app links to, crediting it).
    url text NOT NULL CHECK (char_length(url) <= 2000),
    -- The apply link exactly as the site gives it.
    apply_url text CHECK (char_length(apply_url) <= 2000),
    company_url text CHECK (char_length(company_url) <= 2000),
    location text CHECK (char_length(location) <= 300),
    -- Who may apply, as the site's structured field says.
    regions text[] NOT NULL DEFAULT '{}',
    work_mode text CHECK (work_mode IN ('remote', 'hybrid', 'onsite')),
    salary_min double precision,
    salary_max double precision,
    salary_currency text CHECK (char_length(salary_currency) <= 3),
    salary_period text CHECK (char_length(salary_period) <= 20),
    salary_text text CHECK (char_length(salary_text) <= 300),
    posted_at timestamptz,
    -- Kept only for a site whose terms ask to drop expired jobs.
    expires_at timestamptz,
    -- Plain text, cut at the instance's cap (`APPLICAMP_SOURCES__STORE_DESCRIPTION_CHARS`).
    description text NOT NULL DEFAULT '' CHECK (char_length(description) <= 20000),
    -- ISO 639-1, when the site says; none of the readers does yet.
    language text CHECK (char_length(language) BETWEEN 2 AND 8),
    -- What the site offers (its tags in the registry), as lower-case words
    -- separated by spaces: the text search reads them.
    tags text NOT NULL DEFAULT '',
    -- The keys the search already merges jobs by (dedupe.rs): the apply
    -- link without tracking, and company, role and place.
    url_key text,
    dedupe_key text,
    -- A fingerprint of everything above, to keep only new or changed jobs.
    content_hash text NOT NULL,
    first_seen_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    -- The first read that no longer listed it (a full read); cleared when
    -- the site lists it again.
    closed_at timestamptz,
    -- The text search: title and company first, then the site's tags.
    search tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple'::regconfig, title), 'A')
        || setweight(to_tsvector('simple'::regconfig, company), 'B')
        || setweight(to_tsvector('simple'::regconfig, tags), 'C')
    ) STORED,
    CONSTRAINT store_jobs_site_ref_key UNIQUE (board_id, source_ref)
);

CREATE INDEX store_jobs_posted_idx ON sources.store_jobs (posted_at DESC NULLS LAST)
    WHERE closed_at IS NULL;
CREATE INDEX store_jobs_seen_idx ON sources.store_jobs (last_seen_at);
CREATE INDEX store_jobs_first_seen_idx ON sources.store_jobs (first_seen_at DESC);
CREATE INDEX store_jobs_closed_idx ON sources.store_jobs (closed_at) WHERE closed_at IS NOT NULL;
CREATE INDEX store_jobs_expires_idx ON sources.store_jobs (expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX store_jobs_dedupe_idx ON sources.store_jobs (dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX store_jobs_url_idx ON sources.store_jobs (url_key) WHERE url_key IS NOT NULL;
CREATE INDEX store_jobs_search_idx ON sources.store_jobs USING gin (search);

-- One row per read of a registry row: what it did, what the site answered,
-- and when to read it next. The latest row of a row is its schedule.
CREATE TABLE sources.store_reads (
    id uuid PRIMARY KEY,
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    at timestamptz NOT NULL,
    -- ok: read, kept what was new or changed. not_modified: the site said
    -- nothing changed (a conditional request). partial: read, but not the
    -- whole listing, so nothing was closed. error: it could not be read.
    status text NOT NULL CHECK (status IN ('ok', 'not_modified', 'partial', 'error')),
    -- Jobs the site listed.
    listed integer NOT NULL DEFAULT 0,
    -- Jobs stored for the first time, rewritten because they changed, closed
    -- because the site no longer lists them, and dropped (past their expiry,
    -- or over the limits).
    new_jobs integer NOT NULL DEFAULT 0,
    changed_jobs integer NOT NULL DEFAULT 0,
    closed_jobs integer NOT NULL DEFAULT 0,
    dropped_jobs integer NOT NULL DEFAULT 0,
    -- What the site answered to the first request (200, 304, 429...).
    http_status integer,
    -- Its validators, sent back with the next read.
    etag text CHECK (char_length(etag) <= 500),
    last_modified text CHECK (char_length(last_modified) <= 100),
    -- What went wrong (`unreachable`, `http_500`...), never the answer.
    error_kind text CHECK (char_length(error_kind) <= 100),
    -- Reads in a row that brought nothing new: the more, the less often.
    quiet integer NOT NULL DEFAULT 0,
    duration_ms integer NOT NULL DEFAULT 0,
    next_read_at timestamptz NOT NULL
);

CREATE INDEX store_reads_board_idx ON sources.store_reads (board_id, at DESC, id DESC);
CREATE INDEX store_reads_at_idx ON sources.store_reads (at);
