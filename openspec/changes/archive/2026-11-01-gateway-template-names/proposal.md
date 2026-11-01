# Agent runs start on a real OpenShell gateway

Issue: https://github.com/notacamp/applicamp/issues/41 (P1 go-live)

## Why
Going live on the real gateway (OpenShell v0.1.6), the server could not make the
probe's sandbox template: the gateway refuses a template name longer than 19
characters ("name exceeds maximum length (20 > 19)") and ours was
`applicamp-runs-probe`, 20. The fake runtime in CI has no such limit, so no test
saw it. Every run would have failed with "could not create a sandbox".

Going live also showed an ordering loop: the gateway checks this server's OIDC
issuer before it listens, so after a reboot it can only come up after the
server, and the server made its templates only once, at its own start.

## What changes
- A kind's template is named after the kind as a DNS label of at most 19
  characters (`runs.probe` is `runs-probe`). A longer kind keeps its first
  twelve characters plus six hex digits of a stable hash of the whole kind, so
  two long kinds never share a template.
- When the templates cannot be made at startup, the server tries again every
  30 seconds until the gateway takes them (or the server stops).

## Not changing
- Policies, labels, the sweep and the run lifecycle. Old `applicamp-…`
  templates on a gateway are left alone (none were ever made: the gateway
  refused them).
