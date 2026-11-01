- **Agent runs start on a real OpenShell gateway.** Each kind's sandbox template now has a name
  the gateway accepts (at most 19 characters; `applicamp-runs-probe` was refused), and when the
  gateway comes up after the server, the templates are made as soon as it does, with no restart.
