-- Board requests (#86): asking a job board for permission (or the owner's
-- choice for a whole hiring system) happens outside the app, by email from the
-- owner's own mailbox. This records that it was asked, when, and the answer, so
-- a board that waits on someone else's reply is not lost among the rest.
CREATE TABLE sources.board_requests (
    id uuid PRIMARY KEY,
    board_id uuid NOT NULL REFERENCES sources.job_boards (id) ON DELETE CASCADE,
    -- `permission`: the board's rules ask for written permission first.
    -- `choice`: one decision for every company on a hiring system.
    kind text NOT NULL CHECK (kind IN ('permission', 'choice')),
    asked_at timestamptz,
    -- The owner who sent it (`identity.accounts.id`, by id).
    asked_by uuid,
    answered_at timestamptz,
    answer text CHECK (answer IN ('yes', 'no')),
    note text CHECK (char_length(note) <= 500),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CHECK ((answered_at IS NULL) = (answer IS NULL))
);

-- One open request per board and kind.
CREATE UNIQUE INDEX board_requests_open_idx
    ON sources.board_requests (board_id, kind)
    WHERE answered_at IS NULL;
CREATE INDEX board_requests_board_idx ON sources.board_requests (board_id);
