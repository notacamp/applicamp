# Job sites: one list grouped by what the sites offer, each site On or Off for you

Issue: https://github.com/notacamp/applicamp/issues/94 (epic #85, ADR 0010). Design: the "Job sites"
screen of the approved prototype. Modules: `sources` (a new `features/my_sites` slice, three
endpoints) and the client's `sources` module (the Job sites page at `/boards`).

**Searches are unchanged by this change.** The second part of #89 makes a search read only the
sites a person has On; this change records the choice and gives that part `on_sites` to read.

## Why
Every approved site is read for everybody today. With dozens of sites, and thousands of company
pages behind them, a person should say which sites are theirs, and the app should have said it
first: a nurse in Lisbon has no use for a Ruby site, and someone who works remote only has no use
for a site of office jobs.

## What changes
- **A per-person choice** (`sources.my_sites`): one row per person and site, On or Off, and who set
  it (`app` or `person`).
- **The app sets the defaults.** For each approved site that needs no key, the app turns it On or
  Off from the person's goals (remote or office, where they live, whether they would move) and their
  roles and skills against what the site offers (its tags: kinds of jobs, where, how remote, its
  specialities). It sets them again whenever the list is read, so changed goals and a newly
  approved site are picked up; a choice the person made is never overwritten. Only approved job
  sites count, never hiring systems or company pages.
- **Groups** (a pure rule): Remote, worldwide; Europe; the person's country; By trade; Startups and
  all-jobs sites.
- **`GET /api/v1/my-sites?q=&show=`**: the counts, the groups with each site (name, what it
  offers, typical salary when known, On or Off, needs a key), the sites that do not allow apps (a
  count and names), and how many company pages are also read. **`PUT /api/v1/my-sites/{board_id}`**
  sets a site On or Off as the person's own choice.
- **`POST /api/v1/companies/{id}/undo-not-interested`**: puts a company the person dropped back
  among the suggestions, so the Companies section can offer Undo on every action.
- **The page** (`/boards`): for everyone, the list with a "Find a site" box, one "Show" select, a
  single On or Off button per row, "Add the key" (owners go to the key; others are told the admin
  adds it), the one sentence about sites that do not allow apps, the Companies section (Keep and
  Drop, then "Following" with Stop, each with Undo, no row vanishes) and the count of company
  pages. Owners get "Your sites" and "All sites" (their admin view, unchanged), opening on "All
  sites".

## What the app does by itself / what waits on the person
By itself: turns on the approved sites that fit the person's goals and tags, and turns them off
again when the goals change, unless the person chose. Waiting on the person: turning a site On or
Off (their choice then stays), following a company; and, for the owner, adding a site's key.

## Cost and limits
No model call and no request to any site. One query for the sites, one for the person's choices,
one for the salaries of the last 30 days.

## Non-goals
Searches reading only the On sites (#89, second part), listing company pages one by one, changing
the owner's admin view (#90), choosing sites by hand when the person has no goals yet (the list
shows every site Off until they have answered the goals).
