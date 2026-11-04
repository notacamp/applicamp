-- The import reads every section (#72): when a section came back empty, it is
-- asked for once more; a spending limit or the daily cap can stop that third
-- call. The draft keeps the two answers already paid for, and says what
-- stopped the rest (the problem code), so the review can say so.
ALTER TABLE profile.imports ADD COLUMN IF NOT EXISTS stopped text;
