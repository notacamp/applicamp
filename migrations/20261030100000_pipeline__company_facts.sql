-- Company and job facts for tailored CVs (#34): what the company's own pages
-- say (read once per company, kept 30 days), what the job wants most, and the
-- few questions where the profile is thin. Nothing here is read in the
-- background: every row starts with a click.

-- A company, per account: its site (asked once), and when its pages were last
-- read. The key is the name, lower case, with its blanks squeezed.
CREATE TABLE IF NOT EXISTS pipeline.companies (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    key text NOT NULL CHECK (char_length(key) BETWEEN 1 AND 300),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 300),
    -- The company's own website, an origin (`https://example.com/`).
    site text CHECK (char_length(site) <= 2000),
    -- Where the site came from: the job board gave it, the apply link's host
    -- is it, or the person typed it (which wins over the others).
    site_source text CHECK (site_source IN ('board', 'apply_host', 'person')),
    -- When its pages were last read (the facts are cached 30 days from it).
    read_at timestamptz,
    -- The pages the last read fetched, without their text:
    -- [{url, fetched_at, kind: 'posting' | 'home' | 'linked'}].
    pages jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- Pages left alone at the last read because robots.txt forbids us.
    pages_blocked integer NOT NULL DEFAULT 0 CHECK (pages_blocked >= 0),
    -- Facts the last read left out because their quote was not on their page.
    facts_dropped integer NOT NULL DEFAULT 0 CHECK (facts_dropped >= 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (account_id, key)
);

-- The pages the facts cite, with their text and when they were fetched. Kept
-- only while a fact cites them (removed ones included, so Undo still shows
-- where it came from).
CREATE TABLE IF NOT EXISTS pipeline.company_pages (
    company_id uuid NOT NULL REFERENCES pipeline.companies (id) ON DELETE CASCADE,
    url text NOT NULL CHECK (char_length(url) <= 2000),
    fetched_at timestamptz NOT NULL,
    kind text NOT NULL CHECK (kind IN ('posting', 'home', 'linked')),
    text text NOT NULL,
    PRIMARY KEY (company_id, url)
);

CREATE TABLE IF NOT EXISTS pipeline.company_facts (
    id uuid PRIMARY KEY,
    company_id uuid NOT NULL REFERENCES pipeline.companies (id) ON DELETE CASCADE,
    account_id uuid NOT NULL,
    text text NOT NULL CHECK (char_length(text) BETWEEN 1 AND 300),
    -- Words from the page, verbatim (checked without the model).
    quote text NOT NULL CHECK (char_length(quote) BETWEEN 1 AND 600),
    -- The page it was read on; one of `company_pages`.
    url text NOT NULL,
    -- The text, squashed: a removed fact is not made again by a later read.
    text_key text NOT NULL,
    removed boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL,
    UNIQUE (company_id, text_key)
);

CREATE INDEX IF NOT EXISTS company_facts_company_idx ON pipeline.company_facts (company_id);

-- Every company read that started (its fetches count toward the daily cap,
-- whether or not the model was reached).
CREATE TABLE IF NOT EXISTS pipeline.company_reads (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL,
    company_id uuid NOT NULL,
    started_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS company_reads_day_idx ON pipeline.company_reads (account_id, started_at);

-- What a job wants most, per opportunity, cached until the posting changes.
CREATE TABLE IF NOT EXISTS pipeline.job_briefs (
    opportunity_id uuid PRIMARY KEY REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    account_id uuid NOT NULL,
    -- A fingerprint of the posting text the analysis was made from.
    posting_key text NOT NULL,
    analyzed_at timestamptz NOT NULL,
    -- [{label, quote, level: 'must' | 'nice'}]
    wants jsonb NOT NULL DEFAULT '[]'::jsonb
);

-- The questions where the profile is thin for what the job wants.
CREATE TABLE IF NOT EXISTS pipeline.brief_questions (
    id uuid PRIMARY KEY,
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    account_id uuid NOT NULL,
    seq integer NOT NULL CHECK (seq >= 0),
    question text NOT NULL CHECK (char_length(question) BETWEEN 1 AND 600),
    -- The question, squashed: the same question is not asked twice.
    question_key text NOT NULL,
    -- The position (place in the profile's `work`) the answer is about.
    about integer CHECK (about IS NULL OR about >= 0),
    -- The skill it is about, when it is about one.
    skill text CHECK (char_length(skill) <= 100),
    state text NOT NULL DEFAULT 'open'
        CHECK (state IN ('open', 'answered', 'skipped', 'dont_have')),
    created_at timestamptz NOT NULL,
    UNIQUE (opportunity_id, question_key)
);
