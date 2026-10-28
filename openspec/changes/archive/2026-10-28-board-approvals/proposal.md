# Board approvals: every waiting board can be checked and approved, and says why when it can't

Issue: https://github.com/notacamp/applicamp/issues/26

## Why
Owner, 2026-10-28: "i tried approving swiss dev board or whatever and i cant, it fails
silently". In production SwissDevJobs was a seeded candidate with the config
`{"fields":{}}`, never checked: Approve was disabled with only "Its config doesn't read". Editing
the JSON config answered `400`, and the error sat at the top of the section with a generic
message, out of view. And: "i didnt decline any job boards btw and none of the reasoning for
rejection seems reasonable". Landing.jobs, The Muse, Wellfound and Dice had been seeded as
**declined** on the owner's behalf.

## What changes
- **Seeded candidates come with real configs** where an endpoint was verified (SwissDevJobs'
  `/api/jobsLight`, Net-Empregos' ISO-8859-1 RSS, The Muse's public API), and every seeded
  candidate never checked with its current config is **checked at start** (one request per
  path, robots respected), so Approve is either ready or says why.
- **Keys.** A config can name the API key a board needs (`key`: its parameters, how to get one,
  where). ITJobs.pt and Adzuna say "Needs an API key from <board>: <how>". An owner sets the key
  on the board's peek; it is stored sealed with the instance secret, sent only to the host it
  was set for, never returned (the peek says "Key set").
- **Nothing declined on the owner's behalf.** A migration moves the four seeded declined rows to
  candidate with the research in their note. Facts are **blockers**, shown and blocking
  approval: Landing.jobs' terms forbid automated reading (quoted), Wellfound's and Dice's
  robots rules disallow their job pages (and their terms forbid it). An opinion is a note (The
  Muse: mostly US, old listings). The seed list declines nothing; only an owner does.
- **Errors where you act.** Each action's error shows beside its button with the server's own
  `detail`. A config that does not read is refused naming the field. Why Approve is disabled
  is always shown, every blocker with its detail, unknown ones included.
- **Check proposes a config.** A board with no usable config is probed (its RSS link, `/api`,
  `/jobs/api`, `/feed`, `/rss`, then JSON-LD postings through `/sitemap.xml`); what reads is
  proposed as fields and where each is read from, with "Use this config". The JSON editor moves
  under "Advanced".

## Modules touched
`sources` (registry, seed list, check, reader, boards API, migrations), `server` (the instance
secret reaches the sources module). Client: Boards peek, boards API, 4 languages; the e2e fake.

## Non-goals
- Overriding a fact: a board whose terms forbid automated reading stays unapprovable; the
  owner can only decline it.
- Verifying ITJobs.pt's mapping without a key: the check does once a key is set.
- A model pass to propose a mapping (still off, as in #5).
