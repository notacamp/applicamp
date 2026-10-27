# Decisions needed

Open questions for the owner, one per heading, each with a recommended answer.

## Remotive: its robots.txt disallows the API its terms invite (#5)

Remotive's API terms (remotive.com/api-documentation, and the notice in every answer) invite
developers to share its jobs with a link back, at most 4 fetches a day. Its robots.txt
(read 2026-10-27) disallows `/api/*` and `/*category=` for every crawler. The board check
honours robots.txt, so Remotive is seeded as a candidate it cannot approve.

**Recommended:** keep it blocked, and email Remotive (the contact on its API page)
asking whether the documented API is meant to be exempt from those robots rules. If they say
yes in writing, approve it with a short note in its terms; if not, decline it with that reason.

## Who owns the job boards on the hosted instance (#5)

Approving, declining, pausing and editing boards is limited to the addresses in
`APPLICAMP_SOURCES__BOARD_OWNERS`; with none set, nobody can approve a board from the app (the
seeded approved boards are still read).

**Recommended:** set it to the owner's sign-in address in the hosted instance's configuration
(applicamp-cloud), and nobody else's.
