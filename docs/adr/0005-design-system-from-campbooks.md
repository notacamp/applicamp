# 0005 — The UI is built on @notacamp/ui

**Status:** Accepted · 2026-10-25

## Context
The owner asked to reuse the Campbooks design system instead of building one.
It is token-based (themes are token sets), keyboard-first, translated to four
languages and already covered by Storybook in Campbooks.

## Decision
Applicamp's client depends on `@notacamp/ui` (extracted from Campbooks Next,
notacamp/campbooks-cloud#371, published to GitHub Packages). Applicamp ships
its own Notion-like token set and builds its screens from the package's
components. No Storybook here; the package's stories live in Campbooks.
This replaces the Radix/shadcn + react-intl choice of connect-web-v2 with the
package's own components and i18next.

## Consequences
New generic components go into the package (a Campbooks PR), not into
Applicamp. Consumers set `minimumReleaseAgeExclude: ['@notacamp/*']` so
fresh package releases install.
