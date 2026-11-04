-- When a company's site cannot be read (#73): why, so the CV page can say
-- so, and the person can paste what the site says instead. The pasted text
-- is read like a page (its facts must quote it), kind `pasted`.

ALTER TABLE pipeline.companies
    ADD COLUMN IF NOT EXISTS read_failure text
        CHECK (read_failure IS NULL
               OR read_failure IN ('robots_disallows', 'no_pages_readable', 'site_unreachable'));

ALTER TABLE pipeline.company_pages DROP CONSTRAINT IF EXISTS company_pages_kind_check;
ALTER TABLE pipeline.company_pages
    ADD CONSTRAINT company_pages_kind_check CHECK (kind IN ('posting', 'home', 'linked', 'pasted'));
