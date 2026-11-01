# A cover letter per opportunity, every line traced, with answers to the job's own questions

Issue: https://github.com/notacamp/applicamp/issues/45 (builds on #37 and #38, the tailored CV and its PDF; uses the company facts of #34)
Module: pipeline (`features/cv/letter`), plus the client's pipeline module.

## Why
A CV alone rarely gets a reply, and a generic letter is worse than none. The same rule that makes the CV
trustworthy applies to a letter: every sentence says only what the person's profile (or a fact read on the
company's own site, for why the company appeals) supports, and anything that cannot be traced is shown, not
dropped. Applications also ask their own questions ("What's your notice period?"); the ones the profile cannot
answer must come back empty, for the person, never invented. The prototype was approved by the owner
("keep going, i like it").

## What changes
- **Draft** (one model call, on a click, only with an approved CV): the model is shown the posting, what the job
  wants, the company facts kept, the profile with every kept fact, the words of the approved CV (so the two
  agree), the language, the length (short, about 180 words, the default; standard, about 300) and the job's own
  questions. It never sees a name or contact details: the server prints the greeting, name and contact. It answers
  paragraphs of sentences, each citing sources, and one answer per question. The prompt bans "I am writing to
  apply…", flattery, clichés and superlatives the sources do not support.
- **Fact check, no AI**, the CV's check with the letter's differences: every claim must be in the cited sources;
  a company fact may be cited in any paragraph to say why the company appeals, but a sentence that cites only
  company facts cannot say anything about the person ("I led…" is flagged); superlatives ("best", "go-to",
  "expert", in en/pt/es/fr) are flagged unless a cited source says them; an answer the profile cannot support
  comes back with no text and the flag "Your profile and goals don't say. Answer it yourself", with a box for
  the person's own answer. Flag, never drop.
- **Review** at `/pipeline/$id/letter`: the letter as a page with per-paragraph tags (Company fact, From your
  profile, No claims) and "Where this came from", the job's questions below it, and an aside with the banner, "At a
  glance" (words, matches CV vN, the company facts used) and the spend. Actions: It's true (the person's words
  join the profile as a fact, as on the CV), Edit, Remove; their own answer for a question. Approve stays off
  until nothing is open.
- **Approve** renders the letter with the CV's Typst stack (a letter template in the same style, one page, the
  same embedded fonts, the text as `sys.inputs` data, never markup) and the same read-back (every paragraph
  survives, name and email are text, one page), and keeps it as a version. Answers to the job's questions are
  not in the PDF: text with Copy.
- **Everywhere else**: the CVs page lists letters too (All / CVs / Letters / Drafts); the peek's Tailored CV
  block gets a Cover letter line (draft or approved, Review, View PDF); the CV review page offers "Write the
  cover letter" once the CV is approved.
- The claim check treats "I'm", "I've", "I'd" and "I'll" as the pronoun, not as a name (a letter is full of them).

## Cost
Draft or Make again: one call, about €0.002 (about 5k tokens in, 1k out). Everything else costs nothing: the check,
the PDF, the read-back and the versions. It counts against the AI limits (€0.30 a day, €5 a month). Caps: 10 letter
drafts per opportunity and 20 a day per account, counted apart from CV drafts. Storage: 30 KB per version.

## Non-goals
- Scraping or filling an application form: the job's questions are the lines of the posting's "Application
  questions" section, read without a model. Form scraping is a later issue.
- Sending the letter anywhere, or paying anything: nothing leaves the app.
- A letter without an approved CV, or several letter templates.
- A model to read the questions, a second model to check the letter, or any content heuristic beyond the
  deterministic checks above.
