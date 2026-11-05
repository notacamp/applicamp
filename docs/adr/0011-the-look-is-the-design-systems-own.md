# 0011 — The look is the design system's own

**Status:** Accepted · 2026-11-05 (#78) · amends ADR 0005 (the token set)

## Context
ADR 0005 said Applicamp ships its own Notion-like token set over `@notacamp/ui`:
Notion's greys, a blue accent, the system sans, tighter radii, Notion's tag
colours. In practice the app looked like a fork of the design system rather
than like it, and every blue button broke the system's own rule that there is
no blue default. The owner's review of the design (2026-11-04) asked for the
package's look as published.

## Decision
Applicamp uses `@notacamp/ui`'s tokens as published: Funnel Sans and Funnel
Display (its `fonts.css`), its warm neutrals, its ink accent, its radii and its
signal hues, light and dark alike. `clients/app/src/lib/theme/applicamp.css`
holds only tokens the package has none for: the page widths, the match bar,
the app mark, and the target heights (36px on a desktop, 44px on a phone).
Meaning is carried by words; colour only repeats it, through the package's
signal hues. No accent of Applicamp's own, no blue.

## Consequences
A change of look is a package change (a Campbooks PR), never a token override
here. The Label composition maps its hues onto the package's signal hues. A
screen that needs a colour, size or type the package has no token for asks the
package for one.
