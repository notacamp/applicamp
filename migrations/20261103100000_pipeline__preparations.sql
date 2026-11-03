-- Prepare by itself (#70, ADR 0010): a shortlisted job gets ready, waiting
-- only on the person.

-- One row per opportunity being prepared by itself: the guard against running
-- a step twice, and what the page shows. Ids, states, codes and estimates
-- only.
CREATE TABLE IF NOT EXISTS pipeline.preparations (
    opportunity_id uuid PRIMARY KEY REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    account_id uuid NOT NULL,
    state text NOT NULL CHECK (state IN ('queued', 'running', 'waiting', 'done', 'stopped', 'failed')),
    -- The step to run next, or the one that stopped or failed.
    step text NOT NULL CHECK (step IN ('read', 'draft', 'letter')),
    -- While waiting: what on.
    waiting_on text CHECK (waiting_on IN ('site', 'answers', 'cv_approval', 'letter_approval')),
    -- A problem code when stopped by a cap or a limit, or when a step failed.
    reason text CHECK (char_length(reason) <= 100),
    -- The language the person uses, for the questions (`pt-PT`).
    locale text CHECK (char_length(locale) <= 35),
    -- The steps done by itself: [{step, at, eur}].
    done jsonb NOT NULL DEFAULT '[]'::jsonb,
    updated_at timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS preparations_account_idx ON pipeline.preparations (account_id, state);

-- The facts an answer proposed, so the CV waits until they are kept or dropped.
ALTER TABLE pipeline.brief_questions
    ADD COLUMN IF NOT EXISTS proposals uuid[] NOT NULL DEFAULT '{}';

-- A company's site can be found in the posting (a link or an email address)
-- or from its name, checked against its home page.
ALTER TABLE pipeline.companies DROP CONSTRAINT IF EXISTS companies_site_source_check;
ALTER TABLE pipeline.companies ADD CONSTRAINT companies_site_source_check CHECK (site_source IN
    ('board', 'apply_host', 'person', 'posting_link', 'posting_email', 'name_guess'));

-- When its site was last looked for and not found: not looked for again for
-- 30 days.
ALTER TABLE pipeline.companies ADD COLUMN IF NOT EXISTS site_sought_at timestamptz;
