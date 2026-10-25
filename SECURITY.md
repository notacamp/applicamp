# Security policy

Please **do not open public issues for vulnerabilities**. Report them privately
via [GitHub Security Advisories](https://github.com/notacamp/applicamp/security/advisories/new).
We aim to acknowledge within 3 working days.

Applicamp is pre-release; only the head of `main` is supported.

Areas we care most about: anything that lets an agent reach a host its
sandbox policy doesn't allow, read a credential, act outside its run, or send
or submit something without the user's approval.
