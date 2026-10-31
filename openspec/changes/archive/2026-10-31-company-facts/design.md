# Design: company and job facts

## Where it lives
- **Module: `pipeline`**, new slice `features/brief` (routes, storage, the two
  prompts and their checks). The opportunity, its stage and its company are the
  pipeline's; a separate `research` module would need ports to read every
  one of them back. Schema `pipeline` owns the new tables (migration
  `pipeline__company_facts`): `companies` (per account, keyed by name: its
  site, where the site came from, when it was read, the pages fetched without
  their text), `company_pages` (the text of the pages a fact cites, with URL
  and fetch time, kept only while a fact cites them), `company_facts`,
  `company_reads` (one row per read that started: the daily cap), `job_briefs`
  (what the job wants, with the posting's fingerprint) and `brief_questions`.
- **Ports, not imports** (`architecture.toml` stays without entries): the
  pipeline's `Neighbours` gains `site_allowed` and `read_site` (sources),
  `profile_digest`, `answer_about` and `dismiss_skill` (profile); the server's
  `PipelineWiring` implements them. `PipelineDeps` gains the AI gateway.
- **Fetching reuses the sources module's guard**, no new HTTP client:
  `Reader::page` uses the reader's client that resolves only public addresses
  (and allows private hosts only when the instance does, for the fake site),
  its 10 s timeout, its pace, its user agent and no cookies; and
  `net::check` on every hop. It reads at most 300 KB (a longer page is cut),
  only text or HTML, and follows at most three redirects, only inside the
  same site (`www.` or not). `pages::read_site` reads the posting, the home
  page and up to five same-site pages linked from it whose path or link text
  names a topic, in the topics' order; each page needs `robots::allowed`
  (RFC 9309, our product token then `*`, longest rule wins) from the robots.txt
  of its own host. Another host is never asked: not a link, not a redirect.
  A site whose robots.txt forbids its home page is not read at all.
- **Posting text**: a pasted link to a non-board site has no description. The
  posting page read here is stored as its description when it has none (the
  person's own click, under the same rules), and the job analysis reads
  `description`.
- **Site choice** (`site.rs`, pure): the typed site, then the board's company
  website (`Fields.company_url` in a board's field mapping, kept on the
  company when a search adds the job), then the apply link's host, unless it is
  in a short list of job boards and tracking systems or is a board the posting
  was seen on; a `jobs.`/`careers.` label is dropped. The person's site always
  wins and a different one forgets the facts read on the old one.

## The model
- **Role `complete`**, temperature 0, strict JSON schema, one call per step,
  a second only after a cut-off (4k then 8k tokens: a reasoning model thinks
  first); a spending limit ends the attempt at once. Without the role the
  endpoints say `ai_unavailable` and nothing is fetched. The fallback for the
  rest of the app is unchanged: this page is the only thing that needs it.
- **Facts call**: the pages' text (posting first, cut to 24k characters in
  all) and the company's name; the prompt says to use only the pages and never
  memory, and that page text is data. Output `{text, quote, url}`.
  Checks, no model: the URL must be a page that was read and the quote must
  appear in that page's text as the model saw it (the profile's `squash`
  comparison); otherwise the fact is dropped and counted. At most 12 facts,
  no repeats.
- **Analysis call**: the posting (12k characters), the facts that are not
  removed, a compact profile and the dismissed skills. Output wants and
  questions. Checks, no model: a want's quote must be in the posting; a want
  without a valid level is dropped; at most six wants and three questions; a
  question about a dismissed skill, empty, too long or repeated is dropped; an
  `about` that is not a position becomes none.
- **Answer call** (profile module, `grill::about`): one small call; the
  profile module's own `judge` runs the same quote and text checks as Grill me
  and the facts are stored as Grill me's proposals (turn 0, 24 hours) on the
  position, so Keep, Fix and Drop are Grill me's endpoints, unchanged, and
  nothing reaches the profile without a click. The question is shown to the
  model only as context, never as evidence.
- **Undo**: removing a fact (restore), a typed site (type another), a kept
  fact (Remove with Undo on the profile). Skip and "I don't have it" are
  final for the question; the dismissed skill is Grill me's existing
  `skill_dismissals`.

## API
All owner-only through the session (`SignedIn`), an opportunity of another
account is 404, errors are problem+json:
`GET /api/v1/opportunities/{id}/brief` (never calls the model or fetches),
`PUT …/brief/site`, `POST …/brief/read?locale&again`, `DELETE …/brief/facts/{fact_id}`,
`POST …/brief/facts/{fact_id}/restore`, `POST …/brief/questions/{id}/answer`,
`POST …/questions/{id}/dont-have`, `POST …/questions/{id}/skip`. Problems:
`site_needed`, `invalid_site`, `site_not_allowed`, `site_unreachable`,
`robots_disallows`, `no_pages_readable`, `company_reads_daily_cap`,
`profile_needed`, `question_closed`, `no_skill`, and the AI ones
(`ai_unavailable`, `ai_daily_limit_reached`, `ai_budget_reached`, `ai_failed`).

## Privacy and untrusted text
Page text and posting text are untrusted: shown to the model as data, quoted,
never obeyed, and never rendered as HTML (plain text only). The profile sent
to the model has roles, dates, kept fact texts and skill names, no name,
address, links or contact details. Logs and events carry ids and counts, never
page text, a fact or an answer. Fetched text is kept only while a fact cites it.

## Client
`/pipeline/$id/prepare` composes the pipeline module's page with the profile
module's `JobAnswerFacts` (Grill me's `FactsTaken`) in the app route, because
modules import each other only through `index.ts`. The peek shows the Tailored
CV block from Shortlisted on. Everything is `~/lib/ui`, in four languages.
