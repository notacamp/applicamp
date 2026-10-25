# 0002 — Agents run in NVIDIA OpenShell sandboxes

**Status:** Accepted · 2026-10-25 (provisional until the spike, issue #2, passes)

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
which injects the key outside the sandbox. The gateway runs on its own arm64
host so runaway agents can't starve other services.

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
