-- Hard requirements are pass/fail (#19): the facts a search's model read from
-- each posting about the person's requirements, and why an opportunity is out
-- of view when it fails one (found before the rules, checked once after).
ALTER TABLE pipeline.opportunities
    -- It can be done fully remote; NULL: never checked (found before #19, or
    -- added by link).
    ADD COLUMN remote text CHECK (remote IN ('yes', 'no', 'unclear')),
    -- Someone where the person lives can do it without moving.
    ADD COLUMN open_to_candidate text CHECK (open_to_candidate IN ('yes', 'no', 'unclear')),
    -- Where it is open, as the posting says ("Germany or Austria only").
    ADD COLUMN region_note text CHECK (char_length(region_note) <= 80),
    -- The languages it needs, ISO 639-1.
    ADD COLUMN working_languages text[],
    -- Out of view because it fails a requirement; never deleted.
    ADD COLUMN hidden_reason text
        CHECK (hidden_reason IN ('not_remote', 'not_open_to_location', 'language', 'below_floor'));

-- The recheck looks for Found and Shortlisted ones without facts.
CREATE INDEX opportunities_unchecked_idx ON pipeline.opportunities (created_at DESC)
    WHERE remote IS NULL AND stage IN ('found', 'shortlisted');
