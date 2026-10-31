-- Tell me about you, part 2 (#44): the questions about the gaps, one at a time.
-- What was asked, answered and skipped for this profile version, and the
-- question on screen, live with the rest of the wizard's pending state (same
-- seven days, same sweep, gone at Save).
ALTER TABLE profile.about_you
    ADD COLUMN IF NOT EXISTS questions jsonb NOT NULL DEFAULT '{}'::jsonb;

-- A question's wording and an answer's reading are model calls too, and the
-- wizard's daily cap counts them.
ALTER TABLE profile.about_calls DROP CONSTRAINT IF EXISTS about_calls_kind_check;
ALTER TABLE profile.about_calls
    ADD CONSTRAINT about_calls_kind_check
    CHECK (kind IN ('portrait', 'extra', 'research', 'question', 'answer'));
