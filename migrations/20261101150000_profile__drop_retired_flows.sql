-- The retired Improve, drafts, suggested-skills and Grill me interview flows
-- are gone (#49); so are the tables only they read.
--
--   profile.gap_reviews  Improve's review: its questions and found skills.
--   profile.drafts       the drafts made from an answer to one of its questions.
--   profile.grill_turns  the interview's questions, answers and notes (and
--                        the per-day count of its model calls).
--
-- Kept: profile.skill_dismissals (the questions of "Tell me about you" read
-- it), profile.grill_sessions and profile.grill_proposals (the facts taken
-- from an answer about a job still wait there for Keep, Fix or Drop).

-- Reviews still waiting to run would never be claimed (their kind is no
-- longer registered): remove them.
DELETE FROM kernel.jobs WHERE kind = 'profile.gap_review';

DROP TABLE IF EXISTS profile.gap_reviews;
DROP TABLE IF EXISTS profile.drafts;
DROP TABLE IF EXISTS profile.grill_turns;
