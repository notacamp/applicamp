//! A kind's policy as one run's sandbox gets it.
//!
//! A committed policy names the agent API as `applicamp.example.com:443`
//! ([`API_PLACEHOLDER`]); each run's copy names this instance's agent API
//! instead, so a policy file never holds a real host. The policy is parsed
//! with OpenShell's own parser and set on the sandbox when it is created
//! (a later kind can narrow it to its subject's host the same way).

use openshell_sdk::raw::proto::SandboxPolicy;
use url::Url;

/// The host a policy file names the agent API by.
pub const API_PLACEHOLDER: &str = "applicamp.example.com";
/// And its port.
pub const API_PLACEHOLDER_PORT: u32 = 443;

/// `yaml` parsed, with the agent API placeholder replaced by `api`'s host
/// and port.
pub fn for_run(yaml: &str, api: &Url) -> Result<SandboxPolicy, String> {
    let mut policy = openshell_policy::parse_sandbox_policy(yaml)
        .map_err(|e| format!("the policy does not parse: {e}"))?;
    let host = api
        .host_str()
        .ok_or_else(|| "the agent API address has no host".to_owned())?
        .to_owned();
    let port = u32::from(
        api.port_or_known_default()
            .ok_or_else(|| "the agent API address has no port".to_owned())?,
    );
    for rule in policy.network_policies.values_mut() {
        for endpoint in &mut rule.endpoints {
            if endpoint.host == API_PLACEHOLDER {
                endpoint.host.clone_from(&host);
                if endpoint.port == API_PLACEHOLDER_PORT {
                    endpoint.port = port;
                }
                for p in &mut endpoint.ports {
                    if *p == API_PLACEHOLDER_PORT {
                        *p = port;
                    }
                }
            }
        }
    }
    Ok(policy)
}

/// Every `(host, port)` the policy lets something reach.
pub fn hosts(policy: &SandboxPolicy) -> Vec<(String, u16)> {
    let mut hosts = Vec::new();
    for rule in policy.network_policies.values() {
        for endpoint in &rule.endpoints {
            let ports = if endpoint.ports.is_empty() {
                vec![endpoint.port]
            } else {
                endpoint.ports.clone()
            };
            for port in ports {
                if let Ok(port) = u16::try_from(port)
                    && !endpoint.host.is_empty()
                {
                    hosts.push((endpoint.host.to_ascii_lowercase(), port));
                }
            }
        }
    }
    hosts.sort();
    hosts.dedup();
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_becomes_the_agent_api() {
        let api = Url::parse("http://127.0.0.1:41234").unwrap();
        let policy = for_run(crate::probe::POLICY, &api).unwrap();
        assert_eq!(hosts(&policy), [("127.0.0.1".to_owned(), 41234)]);
        let public = Url::parse("https://applicamp.example.org").unwrap();
        let policy = for_run(crate::probe::POLICY, &public).unwrap();
        assert_eq!(hosts(&policy), [("applicamp.example.org".to_owned(), 443)]);
    }

    #[test]
    fn a_broken_policy_is_refused() {
        let api = Url::parse("https://applicamp.example.org").unwrap();
        assert!(for_run("version: [", &api).is_err());
        assert!(for_run("version: 1\nnot_a_field: true\n", &api).is_err());
    }
}
