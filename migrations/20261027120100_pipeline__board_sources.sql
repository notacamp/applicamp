-- Job boards (#5): an opportunity's source is any job board (its slug), not
-- only the first five; it keeps the apply link as the board gives it, when the
-- posting expires, what dedupes it across boards, and every board it was seen
-- on, with a link to the posting there.
ALTER TABLE pipeline.opportunities DROP CONSTRAINT opportunities_source_check;
ALTER TABLE pipeline.opportunities
    ADD CONSTRAINT opportunities_source_check CHECK (source ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    -- The board's apply link, unchanged.
    ADD COLUMN apply_url text CHECK (char_length(apply_url) <= 2000),
    -- Past this, a Found or Shortlisted one is out of view (the board's terms).
    ADD COLUMN expires_at timestamptz,
    -- The apply link without tracking parameters, and company + role + place:
    -- the same job seen on another board is one opportunity.
    ADD COLUMN canonical_url text CHECK (char_length(canonical_url) <= 2000),
    ADD COLUMN dedupe_key text CHECK (char_length(dedupe_key) <= 700);

CREATE INDEX opportunities_canonical_idx ON pipeline.opportunities (account_id, canonical_url)
    WHERE canonical_url IS NOT NULL;
CREATE INDEX opportunities_dedupe_idx ON pipeline.opportunities (account_id, dedupe_key)
    WHERE dedupe_key IS NOT NULL;

-- Every job board an opportunity was seen on, with the posting's link there:
-- the app credits each one ("From Himalayas").
CREATE TABLE pipeline.opportunity_sources (
    opportunity_id uuid NOT NULL REFERENCES pipeline.opportunities (id) ON DELETE CASCADE,
    board text NOT NULL CHECK (board ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    url text NOT NULL CHECK (char_length(url) <= 2000),
    seen_at timestamptz NOT NULL,
    PRIMARY KEY (opportunity_id, board)
);
