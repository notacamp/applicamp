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
//!   (their terms forbid automation, ADR 0006), no IPv4 host written other
//!   than `a.b.c.d` (`127.1`, `2130706433`, `0177.0.0.1`), no private,
//!   shared (tailnet), loopback, link-local, reserved, multicast or NAT64
//!   address as a host or in `allowed_ips`, and no `allowed_ips` range wider
//!   than /24 (IPv4) or /48 (IPv6).
//!
//! [`lint`] is a pure function over a file's name and text, so the rules
//! are unit tested with fixtures.

use std::net::IpAddr;

/// Hosts no policy may name, with every host under them.
pub const FORBIDDEN_DOMAINS: [&str; 2] = ["linkedin.com", "indeed.com"];

/// The widest IPv4 range `allowed_ips` may name.
pub const MIN_V4_PREFIX: u8 = 24;
/// The widest IPv6 range `allowed_ips` may name.
pub const MIN_V6_PREFIX: u8 = 48;

/// Whether `ip` is an address a sandbox must never reach: private, shared,
/// loopback, link-local, unspecified, reserved, documentation, benchmark,
/// multicast or NAT64.
fn is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // 0.0.0.0/8: "this network".
                || a == 0
                // 100.64.0.0/10: shared address space (carrier NAT, tailnets).
                || (a == 100 && (64..=127).contains(&b))
                // 192.0.0.0/24 (protocol assignments) and the documentation
                // ranges 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24.
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113)
                // 198.18.0.0/15: benchmarking.
                || (a == 198 && (b & 0xfe) == 18)
                // 224.0.0.0/4 multicast and 240.0.0.0/4 reserved.
                || a >= 224
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            let first = segments[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                // 2001:db8::/32: documentation.
                || (first == 0x2001 && segments[1] == 0x0db8)
                // 64:ff9b::/96 and 64:ff9b:1::/48: NAT64, an IPv4 address in
                // disguise.
                || (first == 0x0064 && segments[1] == 0xff9b)
                || v6.to_ipv4_mapped().is_some_and(|v4| is_internal(IpAddr::V4(v4)))
                // ::/96, IPv4-compatible.
                || segments[..6].iter().all(|s| *s == 0)
        }
    }
}

/// The address and prefix length in `raw` (`10.0.0.1`, `10.0.0.0/8`,
/// `[::1]`), if it is one.
fn address(raw: &str) -> Option<(IpAddr, Option<u8>)> {
    let raw = raw.trim();
    let (ip, prefix) = match raw.split_once('/') {
        Some((ip, prefix)) => (ip, Some(prefix.trim().parse().ok()?)),
        None => (raw, None),
    };
    Some((ip.trim_matches(['[', ']']).parse().ok()?, prefix))
}

/// Whether `host` is written like an IPv4 address (only decimal, octal or
/// hex numbers between dots: `127.1`, `2130706433`, `0x7f.0.0.1`), which
/// resolvers read in more ways than one.
fn numeric(host: &str) -> bool {
    !host.is_empty()
        && host.split('.').all(|label| {
            let digits = label.strip_prefix("0x").unwrap_or(label);
            !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_hexdigit())
                && (label.starts_with("0x") || label.chars().all(|c| c.is_ascii_digit()))
        })
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
    if numeric(&host) && host.parse::<std::net::Ipv4Addr>().is_err() {
        return Some("is not an IPv4 address written the one canonical way (a.b.c.d)");
    }
    if address(&host).is_some_and(|(ip, _)| is_internal(ip)) {
        return Some("is a private, loopback, link-local or reserved address");
    }
    None
}

/// Why `range` may not be in `allowed_ips`, if it may not.
fn range_problem(range: &str) -> Option<String> {
    let Some((ip, prefix)) = address(range) else {
        return Some("is not an address or a range".to_owned());
    };
    if is_internal(ip) {
        return Some("is not a public address or range".to_owned());
    }
    let (widest, max) = match ip {
        IpAddr::V4(_) => (MIN_V4_PREFIX, 32),
        IpAddr::V6(_) => (MIN_V6_PREFIX, 128),
    };
    match prefix {
        Some(prefix) if prefix > max => Some(format!("has a prefix longer than /{max}")),
        Some(prefix) if prefix < widest => Some(format!("is wider than /{widest}")),
        _ => None,
    }
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
            for range in &endpoint.allowed_ips {
                if let Some(why) = range_problem(range) {
                    problems.push(format!("{at}: allowed_ips `{range}` {why}"));
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
    fn internal_and_reserved_addresses_fail() {
        for internal in [
            "10.0.0.1",
            "fd00::1",
            "fe80::1",
            "0.1.2.3",
            "198.18.0.1",
            "198.19.255.1",
            "224.0.0.1",
            "240.0.0.1",
            "192.0.2.10",
            "64:ff9b::a00:1",
            "::ffff:127.0.0.1",
            "::127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(
                address(internal).is_some_and(|(ip, _)| is_internal(ip)),
                "{internal}"
            );
        }
        for public in ["93.184.215.14", "2606:2800::1", "198.20.0.1"] {
            assert!(
                !address(public).is_some_and(|(ip, _)| is_internal(ip)),
                "{public}"
            );
        }
    }

    #[test]
    fn non_canonical_ipv4_hosts_fail() {
        for host in [
            "127.1",
            "2130706433",
            "0177.0.0.1",
            "0x7f.0.0.1",
            "0x7f000001",
            "1.2.3.04",
        ] {
            assert!(host_problem(host).is_some(), "{host}");
        }
        for host in [
            "93.184.215.14",
            "api.example.com",
            "deadbeef.example.com",
            "1password.com",
        ] {
            assert_eq!(host_problem(host), None, "{host}");
        }
    }

    #[test]
    fn wide_or_internal_ranges_fail() {
        for range in [
            "1.0.0.0/1",
            "8.8.0.0/16",
            "2606:2800::/32",
            "10.0.0.0/24",
            "nonsense",
            "1.2.3.4/33",
        ] {
            assert!(range_problem(range).is_some(), "{range}");
        }
        for range in ["93.184.215.0/24", "93.184.215.14", "2606:2800:220::/48"] {
            assert_eq!(range_problem(range), None, "{range}");
        }
        let policy = with_endpoint(
            "      - { host: jobs.example.com, port: 443, protocol: rest, enforcement: enforce, \
             access: read-only, allowed_ips: [\"1.0.0.0/1\"] }\n",
        );
        let problems = lint("wide.yaml", &policy);
        assert!(
            problems.iter().any(|p| p.contains("1.0.0.0/1")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_broken_file_fails() {
        assert!(lint("broken.yaml", "version: [")[0].contains("does not parse"));
    }
}
