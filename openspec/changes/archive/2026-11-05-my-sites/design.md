# Design

**The offer, the group, the fit** are pure functions in `features/my_sites/rules.rs`, tested alone.
A site's tags are read into an offer: kinds (the fixed list in `terms.rs`), where (`worldwide`,
`europe`, a region, a country), how remote (`remote`, `some-remote`) and specialities (any other
word; `startups` marks a startup list). A site whose tags were never read (a seeded one) is
taken from its priority: remote-first means remote jobs open worldwide. The registry's country wins
over a tag.

**Group**, the first that fits: specialities, or one or two kinds none of them tech, make By trade;
`startups` makes Startups and all-jobs; the person's country makes Your country; Europe (or a
European country) makes Europe; remote-first and open worldwide makes Remote, worldwide; anything
else is an all-jobs site.

**Fit**, from the goals the search already reads through the `Neighbours` port (`matching_input`;
no other module's schema is read): remote jobs wanted (or no preference) for the remote and
all-jobs groups; Europe when they live in Europe or would move within the EU or anywhere; a
site of office jobs only when they would take office days; By trade when a speciality's words are
among their skills and roles, or, for a narrow trade, when their roles name that kind of work. A
kind that does not match their roles turns a site Off; a person whose roles name no kind matches
every kind.

**Re-running the defaults.** The app writes a default (`set_by = 'app'`) only where there is no row
or the app's own row changed, in one upsert that never touches a `person` row. It runs on every
`GET /api/v1/my-sites` and through `ensure_defaults`, which searches will call. There is no
subscription to the goals event: reading the list is enough, and a search that calls
`ensure_defaults` first sees current goals without one.

**Which rows are listed.** Approved sites; and a waiting site whose only problem is a key it needs
(listed with "Add the key", never On). A site whose terms or robots rules forbid reading, or need
written permission, is named in one sentence. Paused, declined and other waiting sites are not
listed.

**Typical salary.** The average of the engineering jobs a site stated a salary for in 30 days, when
at least ten state one; else its country's typical pay; else none. Rounded to a thousand euros.
