-- Company hiring pages (#87): the registry holds the hiring systems (Greenhouse,
-- Lever, Ashby, Workable, Recruitee, Teamtailor, Personio) as one row each,
-- with the system-level check (robots rules, terms, Content-Signal), and one
-- row per company page found through the apply links of jobs already read.
-- A company page points at its system's row and is approved by the app when
-- that row is approved and a sample read shows recent jobs.

ALTER TABLE sources.job_boards DROP CONSTRAINT job_boards_kind_check;
ALTER TABLE sources.job_boards
    ADD CONSTRAINT job_boards_kind_check CHECK (kind IN
        ('json_api', 'rss', 'jsonld_sitemap', 'hn', 'hiring_system', 'company_page')),
    -- The hiring system a company page (or a system row) is of.
    ADD COLUMN system text CHECK (system IN
        ('greenhouse', 'lever', 'ashby', 'workable', 'recruitee', 'teamtailor', 'personio')),
    -- The company's name on its system (`jobs.lever.co/<token>`).
    ADD COLUMN company_token text CHECK (char_length(company_token) BETWEEN 1 AND 100),
    -- A Lever company on Lever's EU instance.
    ADD COLUMN eu boolean NOT NULL DEFAULT false,
    -- The row of the company page's hiring system.
    ADD COLUMN system_id uuid REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    ADD CONSTRAINT job_boards_pages_check CHECK (
        (kind = 'company_page') = (company_token IS NOT NULL AND system_id IS NOT NULL)
        AND ((kind IN ('company_page', 'hiring_system')) = (system IS NOT NULL))
    );

-- One board per site, except company pages, which share their system's hosts.
ALTER TABLE sources.job_boards DROP CONSTRAINT job_boards_host_key;
CREATE UNIQUE INDEX job_boards_host_idx ON sources.job_boards (host)
    WHERE kind <> 'company_page';
CREATE UNIQUE INDEX job_boards_system_idx ON sources.job_boards (system)
    WHERE kind = 'hiring_system';
-- A company page is unique by system and company (Lever's EU instance apart).
CREATE UNIQUE INDEX job_boards_company_page_idx
    ON sources.job_boards (system, lower(company_token), eu)
    WHERE kind = 'company_page';
-- The daily check takes the oldest company pages not checked yet.
CREATE INDEX job_boards_company_page_due_idx
    ON sources.job_boards (system_id, created_at)
    WHERE kind = 'company_page' AND status = 'candidate';

-- The Brave queries asked, to keep each month within its cap (Brave bills
-- each query). Only the query and how many links came back: nothing else of
-- what Brave answers is kept.
CREATE TABLE sources.web_search_queries (
    id uuid PRIMARY KEY,
    at timestamptz NOT NULL,
    query text NOT NULL CHECK (char_length(query) <= 300),
    results integer NOT NULL DEFAULT 0,
    -- Company pages those links added.
    added integer NOT NULL DEFAULT 0
);
CREATE INDEX web_search_queries_at_idx ON sources.web_search_queries (at);

-- A watched company whose job board is a company page of the registry.
ALTER TABLE sources.companies
    DROP CONSTRAINT companies_board_kind_check,
    ADD CONSTRAINT companies_board_kind_check CHECK (board_kind IN
        ('greenhouse', 'lever', 'ashby', 'workable', 'recruitee', 'teamtailor', 'personio',
         'jsonld', 'none')),
    ADD COLUMN page_id uuid REFERENCES sources.job_boards (id) ON DELETE SET NULL;
