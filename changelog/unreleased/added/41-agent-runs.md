- **Agent runs, each in its own sandbox.** Agents now run in a throwaway NVIDIA OpenShell
  sandbox per run, made when the run starts and always removed when it ends (finished,
  failed, stopped, past its deadline, or left behind by a restart). A sandbox gets only its
  run's id, a short-lived token naming the tools it may call (each with a cap) and the
  server's address; model calls go through the server within your AI limits and the run's
  own token budget. Each kind of run has a committed network policy that blocks everything
  else, and CI rejects a policy that only audits, skips its filesystem rules or names
  LinkedIn, Indeed, a wildcard or a private address. One run at a time per account, three on
  the instance. "Run a check" starts a probe that calls its one tool, is refused a second
  call and sees a site outside its policy blocked; what it did and the sandbox's allow and
  deny lines can be followed live. Self-hosters set `APPLICAMP_SANDBOX__RUNTIME=openshell`
  with the gateway's address (the gateway trusts this server's own keys); without it, runs
  cannot start.
