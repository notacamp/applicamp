//! `cargo xtask policies`: the sandbox policy lint (`AGENTS.md` → "Agent
//! and sandbox rules", ADR 0009).
//!
//! Every file under `policies/` is parsed with OpenShell's own parser and
//! must deny by default (OpenShell does: only listed endpoints are
//! reachable) and
//!
//! - enforce every endpoint it inspects (`enforcement: enforce`; the
//!   default is `audit`, which only logs),
//! - require its filesystem rules (`landlock.compatibility:
//!   hard_requirement`; the default runs without them),
//! - name no wildcard host, nothing under `linkedin.com` or `indeed.com`
//!   (their terms forbid automation, ADR 0006), and no private, shared
//!   (tailnet), loopback, link-local or unspecified address, as a host or in
//!   `allowed_ips`.
//!
//! [`lint`] is a pure function over a file's name and text, so the rules
//! are unit tested with fixtures.

use std::net::IpAddr;

/// Hosts no policy may name, with every host under them.
pub const FORBIDDEN_DOMAINS: [&str; 2] = ["linkedin.com", "indeed.com"];

/// Whether `ip` is an address a sandbox must never reach.
fn is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // 100.64.0.0/10: shared address space (carrier NAT, tailnets).
                || (a == 100 && (64..=127).contains(&b))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| is_internal(IpAddr::V4(v4)))
        }
    }
}

/// The address in `raw` (`10.0.0.1`, `10.0.0.0/8`, `[::1]`), if it is one.
fn address(raw: &str) -> Option<IpAddr> {
    let raw = raw.trim();
    let raw = raw.split_once('/').map_or(raw, |(ip, _)| ip);
    raw.trim_matches(['[', ']']).parse().ok()
}

/// Why `host` may not be in a policy, if it may not.
fn host_problem(host: &str) -> Option<&'static str> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.contains('*') {
        return Some("is a wildcard host");
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Some("is loopback");
    }
    if FORBIDDEN_DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return Some("is a site whose terms forbid automation");
    }
    if address(&host).is_some_and(is_internal) {
        return Some("is a private, loopback or link-local address");
    }
    None
}

/// The lint's findings for the policy file `name` holding `yaml`.
pub fn lint(name: &str, yaml: &str) -> Vec<String> {
    let policy = match openshell_policy::parse_sandbox_policy(yaml) {
        Ok(policy) => policy,
        Err(e) => return vec![format!("{name}: does not parse: {e}")],
    };
    let mut problems = Vec::new();
    let landlock = policy
        .landlock
        .as_ref()
        .map(|landlock| landlock.compatibility.as_str());
    if landlock != Some("hard_requirement") {
        problems.push(format!(
            "{name}: needs `landlock: {{ compatibility: hard_requirement }}` (got {})",
            landlock.unwrap_or("none")
        ));
    }
    let mut rules: Vec<_> = policy.network_policies.iter().collect();
    rules.sort_by(|a, b| a.0.cmp(b.0));
    for (key, rule) in rules {
        for (index, endpoint) in rule.endpoints.iter().enumerate() {
            let at = if endpoint.host.is_empty() {
                format!("{name}: network_policies.{key} endpoint {index}")
            } else {
                format!(
                    "{name}: network_policies.{key} endpoint {index} ({})",
                    endpoint.host
                )
            };
            let enforced = openshell_policy::network_enforcement_mode_to_str(endpoint.enforcement)
                == Some("enforce");
            if endpoint.protocol != "tcp" && !enforced {
                problems.push(format!(
                    "{at}: must say `enforcement: enforce` (the default only audits)"
                ));
            }
            if let Some(why) = host_problem(&endpoint.host) {
                problems.push(format!("{at}: {} {why}", endpoint.host));
            }
            for ip in &endpoint.allowed_ips {
                if address(ip).is_none_or(is_internal) {
                    problems.push(format!(
                        "{at}: allowed_ips `{ip}` is not a public address or range"
                    ));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "\
version: 1
landlock:
  compatibility: hard_requirement
network_policies:
  api:
    endpoints:
      - host: applicamp.example.com
        port: 443
        protocol: rest
        enforcement: enforce
        access: read-write
    binaries:
      - { path: /app/applicamp-agent }
";

    fn with_endpoint(endpoint: &str) -> String {
        format!(
            "version: 1\nlandlock:\n  compatibility: hard_requirement\nnetwork_policies:\n  r:\n    \
             endpoints:\n{endpoint}    binaries:\n      - {{ path: /app/applicamp-agent }}\n"
        )
    }

    #[test]
    fn the_committed_policies_pass() {
        assert_eq!(lint("good.yaml", GOOD), Vec::<String>::new());
        let probe = include_str!("../../../policies/probe.yaml");
        assert_eq!(lint("probe.yaml", probe), Vec::<String>::new());
    }

    #[test]
    fn an_audit_only_endpoint_fails_and_is_named() {
        let audit = with_endpoint(
            "      - { host: api.example.com, port: 443, protocol: rest, access: read-only }\n",
        );
        let problems = lint("audit.yaml", &audit);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].starts_with("audit.yaml: network_policies.r endpoint 0 (api.example.com)")
        );
        assert!(problems[0].contains("enforcement: enforce"));
    }

    #[test]
    fn best_effort_landlock_fails() {
        let soft = GOOD.replace("hard_requirement", "best_effort");
        let problems = lint("soft.yaml", &soft);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("hard_requirement"));
        let none = GOOD.replace("landlock:\n  compatibility: hard_requirement\n", "");
        assert_eq!(lint("none.yaml", &none).len(), 1);
    }

    #[test]
    fn forbidden_hosts_fail() {
        for host in [
            "www.linkedin.com",
            "linkedin.com",
            "uk.indeed.com",
            "*.jobs.example.com",
            "localhost",
            "10.1.2.3",
            "192.168.0.10",
            "100.100.1.1",
            "169.254.169.254",
            "127.0.0.1",
        ] {
            let policy = with_endpoint(&format!(
                "      - {{ host: \"{host}\", port: 443, protocol: rest, enforcement: enforce, access: read-only }}\n"
            ));
            let problems = lint("hosts.yaml", &policy);
            assert!(
                problems.iter().any(|p| p.contains(host)),
                "{host}: {problems:?}"
            );
        }
        let public = with_endpoint(
            "      - { host: jobs.example.com, port: 443, protocol: rest, enforcement: enforce, access: read-only }\n",
        );
        assert!(lint("public.yaml", &public).is_empty());
    }

    #[test]
    fn internal_ranges_fail() {
        for range in ["10.0.0.0/8", "fd00::/8", "fe80::1"] {
            assert!(address(range).is_some_and(is_internal), "{range}");
        }
        for public in ["93.184.215.14", "2606:2800::1"] {
            assert!(!address(public).is_some_and(is_internal), "{public}");
        }
    }

    #[test]
    fn a_broken_file_fails() {
        assert!(lint("broken.yaml", "version: [")[0].contains("does not parse"));
    }
}
