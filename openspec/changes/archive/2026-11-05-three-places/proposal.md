# Three places: Today, Jobs and You; the look is the design system's own

Issue: https://github.com/notacamp/applicamp/issues/78
Module: the client's app shell (`clients/app/src/app`: the frame, the places, the route files) and
`clients/app/src/lib` (the theme file, the Label composition, the shortcut labels). No server change.

## Why
The owner's review of the app (2026-11-04): the design repeats itself and does not guide the person.
Seven places in the sidebar (Today, Pipeline, Boards, Companies, CVs, Agents, Profile & goals) put a
job hunt's plumbing next to its work; every page opens with a paragraph that explains the page; every
nav item and button hints a key, for an app meant for people who are not tech-savvy; and the look
is a fork of the design system (Notion greys, a blue accent, the system sans) rather than the design
system, against its own rule that there is no blue default. The approved proposal keeps three places
and the package's look as published.

## What changes
- **Three places.** The sidebar and the phone tab bar show Today, Jobs (was Pipeline) and You (was
  Profile & goals), in that order, in the four languages. Job sites (`/boards`), Companies, Agents
  and CVs keep their addresses and leave the nav; each marks the place it is reached from as the
  current one (Job sites and Companies under You, Agents under Today, CVs under Jobs), and each stays
  a command in Search (⌘K). Their keys keep working.
- **No key hinted.** Nav items, the search button and the phone header show no key. The sidebar's
  "Keyboard shortcuts" button goes; `?` still lists every key.
- **No page leads.** The paragraph under each place's title goes. Today keeps one line, "Nothing is
  sent without your approval." Empty states say what will appear.
- **The look is the package's.** `applicamp.css` shrinks to the tokens the package has none for: the
  page widths, the match bar, the app mark, and the target heights (36px on a desktop, 44px under
  48rem, for buttons, fields and nav items). Funnel Sans and Funnel Display (the package's
  `fonts.css`), its warm neutrals, its ink accent, its radii and its signal hues come as published,
  light and dark alike. The Label composition's hues map onto the package's signal hues. ADR 0011.
- **Wording.** "Jobs", "You", "Job sites"; "Search" on the search button.

## What the app does by itself, and what waits on the person
Nothing new runs. The page a person is on is marked in the nav by itself, whichever page it is.
Nothing waits on the person.

## Non-goals
- Today's content (one card, Needs you, the activity line and the Agents link): #79.
- The Jobs panel, the Apply stepper, the Setup flow and the You page: #80 to #83.
- The Job sites page's own title and sections: #86 (PR #90).
- Module copy that still says "Pipeline": each module's own issue.
- Removing the key hints inside the ⌘K menu: those are the package's rows.
