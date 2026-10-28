-- Match reasons are grounded in the profile (#27): a reason that claims
-- something the profile and goals do not say is dropped. A score whose every
-- reason was dropped is held below the keep threshold and marked here.
ALTER TABLE sources.scores
    ADD COLUMN ungrounded boolean NOT NULL DEFAULT false;

-- Per search: jobs scored whose every reason was dropped ("not grounded").
ALTER TABLE sources.searches
    ADD COLUMN not_grounded integer NOT NULL DEFAULT 0 CHECK (not_grounded >= 0);
