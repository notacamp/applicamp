-- The words of a listed job's title and description, so how well a board
-- fits someone's skills can be told when they look (#28). Older rows are
-- empty and left out of the fit.
ALTER TABLE sources.board_jobs ADD COLUMN words text[] NOT NULL DEFAULT '{}';
