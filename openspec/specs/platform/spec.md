# platform Specification

## Purpose
What every part of Applicamp shares: the HTTP API described by one generated
OpenAPI document that clients build on, and the app shell people work in.

## Requirements

### Requirement: Every endpoint is in the OpenAPI document
Every HTTP endpoint SHALL be described in the generated OpenAPI document
with an explicit security requirement, and the client SHALL use only types
generated from it.

#### Scenario: An undocumented endpoint
- **WHEN** a route exists that the OpenAPI document does not describe, or the committed document is stale
- **THEN** CI fails

#### Scenario: An operation without a security requirement
- **WHEN** an operation in the document declares neither `security(())` nor the session scheme
- **THEN** CI fails

### Requirement: The app shell
A signed-in person SHALL see three places, Today, Jobs and You, in a sidebar from 48rem and in a
tab bar below it, in their language (English, Portuguese, Spanish or French), in light and dark
themes. Every other page (Job sites, Companies, Agents, CVs, and the pages under a place) SHALL
keep its address, SHALL be reachable from Search and from the place that mentions it, and SHALL
mark the place it is reached from as the current one: Job sites and Companies under You, Agents
under Today, CVs under Jobs. Each place SHALL have a designed empty state that says what will
appear there, and no page SHALL open with a paragraph that explains it; Today says one line,
"Nothing is sent without your approval." No key SHALL be hinted on a nav item, a tab, the search
button or the phone header; the keys keep working and `?` lists them. Nav items, tabs, buttons
and fields SHALL be at least 44px tall below 48rem and 36px from it, and a tab's label at least
12px. The look SHALL be the design system's as published (Funnel Sans and Funnel Display, its
neutrals, its ink accent, its radii, its signal hues); the app's own tokens SHALL be only those
the package has none for (the page widths, the match bar, the app mark, the target heights).

#### Scenario: First visit after sign-in
- **WHEN** a person signs in for the first time
- **THEN** Today shows that nothing needs them yet and offers to set up their profile

#### Scenario: Someone not on the list
- **WHEN** the server sends a browser to `/not-invited`
- **THEN** it shows "You're not on the list yet" and a way back to the sign-in page

#### Scenario: A page that is not a place
- **WHEN** a person opens Agents, from Search or from a page that links to it
- **THEN** the page shows at its address, Today is marked current in the sidebar and the tab bar, and Agents is in neither

#### Scenario: No key hinted
- **WHEN** the sidebar is hovered or a nav item is focused
- **THEN** no key is shown beside any item, the search button or in the phone header

#### Scenario: Big enough to tap
- **WHEN** the screen is narrower than 48rem
- **THEN** every nav item, tab, button and field is at least 44px tall; from 48rem, at least 36px

### Requirement: Longer writing has its own model
An instance MAY configure a `write` model (`APPLICAMP_AI__WRITE__…`, the same settings as
`complete`) for longer writing; the tailored CV SHALL be drafted on it when it is set, and on the
`complete` model when it is not. Its calls SHALL be metered in the same ledger with their role, under
the same daily cap and spending limits, and refused before they would cross a limit.

#### Scenario: One model
- **WHEN** an instance sets only the `complete` model and someone drafts a CV
- **THEN** the CV is drafted on the `complete` model

#### Scenario: Two models
- **WHEN** an instance sets a `write` model too
- **THEN** the CV is drafted on it, and the ledger records the call under `write`
