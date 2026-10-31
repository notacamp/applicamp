# 0002 — Agents run in NVIDIA OpenShell sandboxes

**Status:** Accepted · 2026-10-25 (spike #2 passed on v0.1.4) · amended by [0009](0009-agent-harness.md): model calls go through the server, not OpenShell's router (removed in v0.1.0)

## Context
Agents browse untrusted job pages, fill forms in a real browser and read
recruiter email. They must not be able to reach anything but what their job
needs, must never hold credentials, and must leave nothing behind. It must run on
ordinary arm64 or amd64 cloud VMs with Docker Compose: no GPU, no KVM, no Kubernetes.

## Decision
Run every agent in an **NVIDIA OpenShell** sandbox (Apache-2.0, 0.1.x pinned),
one sandbox per run, created through the gateway's gRPC API from the
`runs` module and always deleted at the end. Use the **Docker driver**
(Landlock + seccomp user notification; Linux 6.2+). Each run kind has a
committed deny-by-default policy. Model calls go through OpenShell's router,
which injects the key outside the sandbox.

The gateway may share a host with other services (the hosted instance runs
next to Campbooks, owner's call 2026-10-25), so it runs **rootless** (rootless Docker; OpenShell's
Podman driver needs Podman 5, newer than Ubuntu 24.04 ships) under its own
unprivileged user, never through a root Docker daemon, inside a
systemd slice with hard CPU and memory caps and a cap on concurrent sandboxes.
Sandboxes have no network route to the host's other containers.

Pre-check (2026-10-25) on our arm64 cloud VMs: Ubuntu 24.04, kernel 6.8,
Landlock on, Docker 29.9, no /dev/kvm. MicroVM mode is therefore unavailable.

## Alternatives
- **gVisor (runsc) + Docker**: works without KVM on arm64. Fallback if the
  spike fails; we would write the egress allow-list proxy and log pipeline ourselves.
- Firecracker, Kata, e2b, microsandbox: need KVM, which typical cloud VMs lack.
- NemoClaw: alpha, built for always-on OpenClaw assistants, not per-task runs.
- NeMo Agent Toolkit: an agent framework, provides no isolation.

## Consequences
OpenShell is pre-1.0: pin the version and keep the gateway behind our own
`SandboxRuntime` trait so the gVisor fallback is a second implementation, not a rewrite.

## Spike findings (issue #2)
- Workloads run with `network=none`, non-root, seccomp filtering and no new
  privileges; only the supervisor (the egress proxy) has a network. Policies
  are enforced per host, per HTTP method (L7) and per binary.
- Docker sandboxes need launch-scoped auth: the gateway is configured with
  Ed25519 sandbox-session keys (`gateway_jwt`).
- Rootless Docker 29's host networking is the host's real network namespace,
  so on a shared host a firewall guard on the sandbox user's uid and subuid
  range is part of the boundary: no private, tailnet, link-local or loopback
  destinations except the gateway's own ports.
- The Rust SDK (`openshell-sdk`, git tag) covers create, wait, exec, delete
  and templates; one **sandbox template per run kind** carries its policy and
  limits; log streaming uses the raw gRPC client. It authenticates with TLS +
  OIDC bearer tokens, so the production gateway trusts the Applicamp server as
  its OIDC issuer.
