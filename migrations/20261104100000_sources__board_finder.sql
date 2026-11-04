-- The board finder (#75): boards approve themselves when their check is
-- clean, their terms are read by a model, and what each board offers is
-- tagged. `approved_by` was the approving account's id; it is now who
-- approved ('owner' or 'app'), and `approved_at` (already there) says when.
-- Keep who approved: the account (by id, no foreign key across schemas).
ALTER TABLE sources.job_boards ADD COLUMN approved_account uuid;
UPDATE sources.job_boards SET approved_account = approved_by;

ALTER TABLE sources.job_boards
    ALTER COLUMN approved_by TYPE text USING (CASE WHEN approved_by IS NULL THEN NULL ELSE 'owner' END);

ALTER TABLE sources.job_boards
    ADD CONSTRAINT job_boards_approved_by_check CHECK (approved_by IN ('owner', 'app')),
    -- What it offers, as plain lowercase words: the kinds of jobs, where,
    -- how remote, and its specialities (read from its sample jobs).
    ADD COLUMN tags text[] NOT NULL DEFAULT '{}',
    -- What its terms say about automated reading, read by a model:
    -- allow, silent, forbid, ask_first or personal_only; NULL when not read.
    ADD COLUMN terms_verdict text CHECK (terms_verdict IN
        ('allow', 'silent', 'forbid', 'ask_first', 'personal_only')),
    -- The sentence of its terms that decided it, verbatim.
    ADD COLUMN terms_quote text CHECK (char_length(terms_quote) <= 600),
    ADD COLUMN terms_url text CHECK (char_length(terms_url) <= 500),
    -- The robots.txt Content-Signal line, as {"search": "yes", "ai-train": "no"}.
    ADD COLUMN content_signal jsonb,
    -- Where to ask it for permission, when its terms or contact page give one.
    ADD COLUMN contact_email text CHECK (char_length(contact_email) <= 254),
    -- How the app found it, in plain words.
    ADD COLUMN found_through text CHECK (char_length(found_through) <= 300);

-- Boards approved before this change were approved by the owner.
UPDATE sources.job_boards
   SET approved_by = 'owner'
 WHERE approved_by IS NULL AND approved_at IS NOT NULL;
