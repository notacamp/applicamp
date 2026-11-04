# Your LinkedIn: a check-up that says what to change on LinkedIn for the jobs you want

Issue: https://github.com/notacamp/applicamp/issues/84
Module: profile (`features/linkedin`), plus the client's profile module and one route.

## Why
A LinkedIn profile is what a recruiter sees first, and it is usually older and thinner than what
Applicamp knows: the profile with the facts kept from Tell me about you, and the goals. People who
are not writers do not know what a headline should say for the role they want, or that their About
text says nothing about it. The app already reads a LinkedIn data export and a LinkedIn profile
saved as PDF (the import), and already has the rule that keeps AI text honest (every line cites its
sources and a check without a model flags what they do not say). A check-up puts the two together:
the AI reads the LinkedIn profile the person gives it, compares it with the profile and the goals,
and proposes what to change, part by part; the person keeps or drops each proposal and ticks it off
once changed on LinkedIn. Nothing is fetched from LinkedIn (ADR 0006) and the app never writes
there: it hands over text to paste.

## What changes
- **A page, "Your LinkedIn"** (`/profile/linkedin`), reached from Profile & goals. With no profile
  yet it says to add one first. Without an AI model set up it says the check-up needs one.
- **The upload**: the LinkedIn data export ZIP (read on the server, no model) or the profile saved as
  PDF (read by the model, the import's two calls), as the import accepts them. The file is not
  stored; the LinkedIn profile as read is kept with the review, without the person's name, email,
  location or links, and with email addresses and phone numbers in its texts replaced.
- **One model call** writes the review: a short overall read (how the LinkedIn profile reads to a
  recruiter for the wanted roles), then one finding per part (headline, About, each position,
  skills, education, languages, certifications, projects) marked Good, Could be better or Missing,
  each with one plain sentence why; and proposals for the headline, the About text, a position's
  description and skills to add, each citing the profile sources, the kept facts or the LinkedIn
  text it comes from, and saying what it does for the goals. The model is shown the profile and goals
  the way Tell me about you shows them (no name, no contact details) and told that everything is
  data, not instructions.
- **The check, no AI**: every proposal's claims must be in the sources it cites (the claim check CVs
  and letters use), and a buzzword ("passionate", "results-driven", "guru", "expert"… in en/pt/es/fr)
  is flagged unless a cited source says it. A flagged proposal stays on the page with its flags and
  cannot be kept until it is edited to say only what its sources say, or dropped. Nothing is
  dropped by the app.
- **Keep, Edit, Drop, Done**: each proposal has Keep and Drop, and Edit (checked again against the
  same sources). Kept proposals make a short list, "To change on LinkedIn", each with its text and a
  Copy button, ticked Done once changed. Every decision can be taken back. Start over throws the
  review away; Check again takes a new file. A review lives 30 days after its last touch.
- **Stale**: when the profile changed since the review, the page says so and offers to check again;
  nothing blocks.
- **Profile & goals** gets a "Your LinkedIn" section with the way in, and how many things wait to
  be changed on LinkedIn when a review exists.

## Cost
One check-up from the ZIP is one call, about €0.003 (about 6k tokens in, 1.5k out); from a PDF,
three (the import's two, then the review), about €0.006. It counts against the AI limits. Cap: 10
check-ups per account per UTC day. Storage: about 30 KB per review, one per account.

## Non-goals
- Reading LinkedIn itself, signing in to it, or changing anything there: the app hands over text
  to paste (ADR 0006).
- Writing a proposal into the Applicamp profile: the profile is edited in place or through Tell me
  about you; a check-up only reads it.
- Rewriting education, languages, certifications or projects: the review says their state and why,
  with no text to paste.
- A second model to check the first, or any content heuristic beyond the claim check and the
  buzzword list.
- Reviewing a profile pasted as text, or a CV: the check-up is about LinkedIn as it is now.
