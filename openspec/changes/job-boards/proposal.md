# Search job boards, and keep finding new ones

Issue: https://github.com/notacamp/applicamp/issues/5

## Why
Owner, 2026-10-27: "we'll have to base ourselves on job boards and we should
be looking for them"; remote-first boards yes, a curated company list no ("at
least for now, later we can have watchers on company boards", #20). The first
search read 107 hand-picked company boards plus a German-heavy aggregator.

## What changes
- Search reads **job boards** through three generic adapters (one per access
  type, configured per board, not code per board): **JSON API with a field
  mapping**, **RSS/Atom with a mapping**, and (later) **JSON-LD JobPosting via
  sitemap**. HN "Who is hiring" keeps its LLM extractor.
- First boards (research 2026-10-27): **Himalayas, Jobicy, Remotive,
  RemoteOK, Arbeitnow** (remote only), **HN**. Each with its terms honoured
  in config: poll interval, delay, attribution text and link, apply-link
  rule, expiry.
- The 107 company boards are **switched off** in search (kept for watchers,
  #20).
- **Board discovery**: a weekly job collects candidate boards (a CC0 list of
  job boards on GitHub, RSS autodiscovery, partner pages of approved
  boards), checks each automatically (robots.txt, terms keywords with the
  matched sentences, feed/API/JSON-LD presence, volume, remote/EU/PT share,
  language), and puts it in an **approval queue**. Nothing is read until the
  owner approves it on the new **Boards** page; approved boards get an
  adapter config proposed from the check, editable before approval.
- **Attribution in the UI**: every opportunity shows its source board with a
  followed link to the original posting; the apply link is the board's,
  unchanged.
- **Dedupe across boards** (canonical apply URL, then company+title+location),
  one merged opportunity listing every source.

## Modules touched
`sources` (adapters, board registry, discovery, scheduling), `pipeline`
(multi-source attribution), client: Pipeline peek, new Boards page (in Profile &
goals' neighbourhood → sidebar "Boards" under Pipeline).

## Non-goals
- Company watchers (#20). Boards that forbid automated reading (Landing.jobs)
  or need a licence (Adzuna commercial) — recorded as "declined" in the registry
  with the reason.
- Sandboxed discovery agents (discovery here is a plain server job; it moves
  to agent runs later).
