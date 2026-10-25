# 0006 — Jobs come from public boards; no LinkedIn or Indeed automation

**Status:** Accepted · 2026-10-25

## Decision
Read jobs only from sources that publish them for this use: Greenhouse,
Lever and Ashby public job-board APIs, SmartRecruiters, Arbeitnow, Adzuna (key),
HN "Who is hiring" via Algolia, Remotive and RemoteOK (with their attribution
and delay terms). Never scrape or automate LinkedIn or Indeed (LinkedIn User
Agreement §8.2; hiQ v. LinkedIn ended in a contract judgment). LinkedIn is used
for sign-in and the user's own data export only.

## Consequences
Coverage is the companies that use those boards; users can add any job by
link. Each source records its rate limit and attribution rules in `sources`.
