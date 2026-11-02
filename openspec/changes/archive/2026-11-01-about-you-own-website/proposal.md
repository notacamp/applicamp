# Tell me about you reads your own website

Issue: https://github.com/notacamp/applicamp/issues/60

## Why
The owner: "can it also read my personal website i passed on". The profile holds the link, but
only past employers' websites were read, so what the person's own site says about their work
(projects, tools, writing) never reached the portrait.

## What changes
- When the portrait is made, the profile's own link is read **alongside the employers**, within
  the same time window: its home page and up to five pages it links to about the person (about,
  projects, work, portfolio, experience, résumé, talks, writing, blog), on the same host only,
  where its robots.txt allows, through the same reader and host guard. Never a search, never
  another host.
- **One model call** picks out at most eight facts about the person's work from at most four
  pages, each with the page's exact words; a fact is kept only when its quote is on the page it
  names. Page text is data, never instructions.
- The facts reach the portrait only as **assumptions to confirm** ("I assume, from your website,
  that you…"), linking their page, because a site can be old. A confirmed one is saved as its plain
  claim, as any confirmed assumption is.
- A site that cannot be read, a model call that fails, or the daily cap nearly reached leaves the
  portrait without it. The read counts toward the daily cap as an employer's does.

## Not changing
- Employers' research, the portrait's time limit, and what is saved.
