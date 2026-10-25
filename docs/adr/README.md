# Architecture decision records

| # | Decision |
|---|---|
| [0001](0001-spec-driven-development.md) | Spec-driven development with OpenSpec, one PR per change |
| [0002](0002-agents-in-openshell-sandboxes.md) | Agents run in NVIDIA OpenShell sandboxes, one per run |
| [0003](0003-no-credentials-in-sandboxes-approvals.md) | Sandboxes hold no credentials; outward actions need an approval |
| [0004](0004-mail-calendar-sso-through-campbooks.md) | Mail, calendar and SSO come from Campbooks |
| [0005](0005-design-system-from-campbooks.md) | The UI is built on `@notacamp/ui` |
| [0006](0006-jobs-from-public-boards-only.md) | Jobs come from public boards; no LinkedIn or Indeed automation |
| [0007](0007-tailored-cvs-typst.md) | Tailored CVs: JSON Resume data, fact-check gate, Typst PDFs |
| [0008](0008-stack.md) | Rust modular monolith + slim React client |
