# Design: the tailored CV as a PDF

## Rendering
`features/cv/pdf` in the pipeline module. `typst`, `typst-layout` and
`typst-pdf` 0.15 with a `World` that serves one embedded template (`clean.typ`
or `compact.typ`), no other file (`AccessDenied`), no packages, no clock, and
the Libertinus Serif fonts of `typst-assets` compiled in. The CV is JSON in
`sys.inputs.cv`, read by the template with `json(bytes(...))`: strings are only
ever shown as text, so `#import`, `]`, `$`, `*`, `_`, `<x>`, `@ref` come out
literally (a test reads them back). Nothing is letter-spaced or uppercased.
The compile runs in `spawn_blocking` under a 10 s timeout and a 2-permit
semaphore; the permit travels with the blocking task. The document is capped
at 256 KB.

## Read-back
`pdf-extract` (already a dependency) gives the text; `lopdf` (its re-export)
counts pages, link annotations and images. Headings must be on a line of
their own as whole words; lines are compared after removing white space, soft
hyphens, ligatures and joining hyphenated line breaks; keywords use the same
matching as the review; name and email must be text and links real URI
annotations. Results are stored as `checks` with the version.

## Versions and atomicity
`cv_versions` rows (account, opportunity, draft, version, template, length,
language, pdf bytea, size, pages, checks, created, downloaded). Approve renders
and checks first (no lock held), then in one transaction freezes the draft and
inserts the version; a refusal leaves the draft untouched. Version numbers come
from the larger of cv_versions and approved drafts, under an advisory lock.
A layout change inserts the next version from the newest approved draft. The
PDF endpoint is a plain 404 for another account, `no-store`, `nosniff`, with an
ASCII `filename` and a UTF-8 `filename*`. Download sets `downloaded_at`.

## Deleting
Opportunities are only dismissed ("Not for me"), never deleted, so versions
stay (a PDF is what was sent); `ON DELETE CASCADE` removes them if an
opportunity row is ever deleted. `/api/v1/cvs` lists dismissed jobs' CVs too.

## Dependencies and licences
`deny.toml` allows BSD-2-Clause, Apache-2.0 WITH LLVM-exception and 0BSD (Typst's
tree). Typst's tree also carries unmaintained or advisory-flagged crates for
features the CV never reaches (bibliographies, highlighting, XML): these are
ignored in `deny.toml`, each with a reason, because no input of ours reaches
them (the World serves no files). The fonts are OFL/Libertinus via typst-assets.

## Client
`PdfViewer` in `~/lib/ui`: PDF.js is `import()`ed on first open and its worker
is a hashed file of our own build (`?url`), no CDN. Pages are drawn to canvases
at the device pixel ratio, fitted to the width. A dialog (full screen below
48rem). `RowList` is a list whose rows hold several buttons. `/cvs` is in the
sidebar and ⌘K (G then C) but not in the phone tab bar, which has room for five.
