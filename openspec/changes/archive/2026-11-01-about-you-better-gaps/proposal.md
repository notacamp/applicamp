# Tell me about you asks what matters for a thin profile

Issue: https://github.com/notacamp/applicamp/issues/57

## Why
On the hosted instance, a profile imported from a PDF held 13 positions with titles and dates
only, 4 skills, and no languages, education, summary or headline. Tell me about you would first
ask for something that changed because of the person's work in a role the profile says nothing
about, never asked about languages (which searches check each posting against) or education, and
never questioned the 4-skill list, because it only finds skills the roles name.

## What changes
- **A role nobody described is asked about first**: what the person did there, then what changed
  because of it. A role with a summary, highlights or kept facts is asked as before.
- **New gaps**, all decided in code like the others: no languages (early, naming the languages the
  goals say they work in), no education (after the summary and headline), and a short skills list
  (fewer than six, with nothing more named in the roles).
- **New changes**: `language` (one per change, `Portuguese (native)` saved as the language and its
  fluency) and `education` (one per change, `BSc in Computer Science, University of Lisbon` saved
  as the course and the school, or `Self-taught`), reviewed under Languages and Education.
- The wizard's rules stand: nothing that can be looked up is asked, every question says why,
  results offer choices and a number is optional, flags are shown and never dropped.

## Not changing
- The limits (12, 8, 16 questions), skip and enough, the review and Save.
