# The check follows a site's redirects within the site

Issue: https://github.com/notacamp/applicamp/issues/115
Module: `sources` (`features/discovery`, `readers`, the board report). One new optional report field,
`moved_to`; no endpoint changes and no migration (the report is jsonb).

## Why
Found in prod on 2026-11-09, in the first discovery run with #114: 100 checks, 12 approved, and 64 left
waiting with "no list of jobs found" (28 of the 50 re-checks, 36 of the 50 new finds). The check reads every
page through `Reader::visit_noting`, whose client never follows a redirect, so a redirect counted as a failed
read and the log line did not say which address it was. In that run the redirect answers were 301 x85, 308 x48,
307 x15 and 302 x9, 157 in all: a homepage that moved to `www.`, the bare domain or a language path, `/feed`
going to `/feed/`, a sitemap that moved. When the homepage itself redirected, the check had no HTML, so it could
not find feed links, a platform, the Jobs page or the terms link.

## What changes
- **Redirects inside the site are followed during a check.** Every page the check reads (robots.txt, the
  homepage, the terms, feeds, sitemaps, API paths) follows a redirect to the same site (`readers::same_site`,
  `www.` or not), at most 3 hops. Each hop is one request of the check's budget, goes through the same public
  address check, and must be allowed by the site's robots rules (robots.txt itself, read before any rules
  exist, needs only to stay on the site). A redirect to another site is not followed.
- **The check carries on from the final address.** After the homepage, links, the terms link and the finder use
  where the homepage ended up; when that is another host (`www.`), its robots.txt is read and its rules apply.
  A config the check proposes or applies names the final address of the feed, API or sitemap, so later reads of
  an approved site, which never follow redirects, read it directly. An owner's edited config is never
  rewritten, and a seeded board's is only read through the redirect.
- **A move to another site is recorded.** The report gets `moved_to` (the other site's host) when the homepage
  goes to another site. Data only, not shown on the admin page yet; the waiting reason stays as it is.
- **The log says what failed.** "a page of a board's check could not be read" carries the host, the path and the
  HTTP status, never the query (it can hold a board's key).
- `RULES` goes from 1 to 2, so #113's pass re-checks every candidate under the new rules, oldest first.

Store reads and search reads still never follow a redirect; only the check's reads do.

## Automates, and what still waits on the person
The app already ran these checks by itself; now they reach sites whose address or feed moved. Limits stay:
the request budget, robots.txt, the address check. Nothing new waits on the person.
