# Where the matches came from counts every match once, company pages included

Issue: https://github.com/notacamp/applicamp/issues/107 (follow-up of #89 / #106)

Module: `sources` (search, the Searches tab in `clients/app/src/modules/sources`).

## Why
On the Searches tab, "Where the matches came from" listed only job sites. A match whose job also came
from a company page was counted under the site that listed it, a company page's match under no line at
all, and a job on two sites under both. In the recording of #106, 16 good matches showed as 9 + 5 = 14.

## What changes
- **Every good match is credited to exactly one source**: a job site, or a company's own hiring page
  (a company page of the registry, or a watched company's job board). The list adds up to the good
  matches.
- **The company page wins.** When the same job came from several places, the copies merge into one
  match that keeps every place it came from (the other sites' links are credited on the opportunity);
  the credit goes to the company page when one is among them, because it is the employer's own listing,
  else to the site that listed it first.
- **Only what was added counts.** A match the pipeline already had is not a good match of this search
  and is counted under "Seen before", as before.
- **Company pages show by name**, "Wayfarer Co’s hiring page" (in four languages), instead of "Company
  sites". When many pages give one or two matches each, the list shows the five biggest sources and then
  "N company hiring pages" (and "N other job sites") as one line each, so it stays short and adds up.
- The scan lines say whether a line is a company page (`company_page` on `ScanLine`); a company page
  that gave a match gets a line of its own under "Show by site".

## Impact
`Neighbours::add_found` says which of the found jobs were new. No migration.
