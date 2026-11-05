-- What a posting asks of where the person is, beyond the region (#111): the
-- hours or time zones it needs, the right to work and visa sponsorship, and
-- moving for the job, as the scoring call read them:
-- {time_zones, work_permit, relocation}, each only when the posting says it.
-- Shown to the person, never used by the rules, so the rules' version is
-- unchanged: scores cached before this have none.
ALTER TABLE sources.scores ADD COLUMN place_notes jsonb;
