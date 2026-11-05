-- What a posting asks of where the person is, beyond the region (#111), as
-- the search read it. NULL: it does not say, or it was found before. Shown
-- in the pipeline; never used to hide an opportunity.
ALTER TABLE pipeline.opportunities
    -- The hours or time zones it needs ("CET ± 2 hours").
    ADD COLUMN time_zone_note text CHECK (char_length(time_zone_note) <= 80),
    -- The right to work and visa sponsorship ("EU work permit, no visa sponsorship").
    ADD COLUMN work_permit_note text CHECK (char_length(work_permit_note) <= 80),
    -- Moving for the job ("Move to Berlin; help offered").
    ADD COLUMN relocation_note text CHECK (char_length(relocation_note) <= 80);
