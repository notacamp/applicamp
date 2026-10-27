//! robots.txt, as RFC 9309 reads it: the group for our product token
//! (`applicamp`), else the one for `*`; the longest matching rule decides,
//! and `Allow` wins a tie; `*` matches anything and a final `$` anchors the
//! end. A missing file (404) allows everything.

use crate::job_boards::{RobotsReport, RobotsRule};

/// Our product token.
pub const AGENT: &str = "applicamp";
/// The most rules kept.
const MAX_RULES: usize = 500;

/// The rules `text` has for us, and the crawl delay it asks for.
pub fn rules_for_us(text: &str) -> (Vec<RobotsRule>, Option<f64>) {
    // (agents, rules, delay) per group.
    let mut groups: Vec<(Vec<String>, Vec<RobotsRule>, Option<f64>)> = Vec::new();
    let mut in_agents = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "user-agent" => {
                if !in_agents {
                    groups.push((Vec::new(), Vec::new(), None));
                    in_agents = true;
                }
                if let Some(group) = groups.last_mut() {
                    group.0.push(value.to_ascii_lowercase());
                }
            }
            "allow" | "disallow" => {
                in_agents = false;
                if let Some(group) = groups.last_mut()
                    && !value.is_empty()
                    && group.1.len() < MAX_RULES
                {
                    group.1.push(RobotsRule {
                        allow: key == "allow",
                        pattern: value.chars().take(300).collect(),
                    });
                }
            }
            "crawl-delay" => {
                in_agents = false;
                if let Some(group) = groups.last_mut() {
                    group.2 = value
                        .parse::<f64>()
                        .ok()
                        .filter(|d| d.is_finite() && *d >= 0.0);
                }
            }
            _ => {
                in_agents = false;
            }
        }
    }
    let ours = |agent: &str| agent == AGENT;
    let pick = |test: &dyn Fn(&str) -> bool| {
        let mut rules = Vec::new();
        let mut delay = None;
        let mut any = false;
        for (agents, group_rules, group_delay) in &groups {
            if agents.iter().any(|a| test(a)) {
                any = true;
                rules.extend(group_rules.iter().cloned());
                delay = delay.or(*group_delay);
            }
        }
        any.then_some((rules, delay))
    };
    pick(&ours)
        .or_else(|| pick(&|agent: &str| agent == "*"))
        .unwrap_or_default()
}

/// Whether `pattern` matches `path` (from its start).
fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            at = part.len();
            continue;
        }
        let last = i == parts.len() - 1;
        if last && anchored {
            return path.len() >= at + part.len() && path.ends_with(part);
        }
        match path[at..].find(part) {
            Some(found) => at += found + part.len(),
            None => return false,
        }
    }
    !anchored || at == path.len()
}

/// Whether `rules` allow `path` (with its query), and the rule that
/// decided ("Disallow: /api/*"), if one did.
pub fn allowed(rules: &[RobotsRule], path: &str) -> (bool, Option<String>) {
    let mut best: Option<&RobotsRule> = None;
    for rule in rules.iter().filter(|r| matches(&r.pattern, path)) {
        let better = match best {
            None => true,
            Some(b) => {
                rule.pattern.len() > b.pattern.len()
                    || (rule.pattern.len() == b.pattern.len() && rule.allow && !b.allow)
            }
        };
        if better {
            best = Some(rule);
        }
    }
    match best {
        None => (true, None),
        Some(rule) => (
            rule.allow,
            Some(format!(
                "{}: {}",
                if rule.allow { "Allow" } else { "Disallow" },
                rule.pattern
            )),
        ),
    }
}

/// The path and query of `url`, as robots rules match it.
pub fn path_of(url: &url::Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    }
}

/// A fingerprint of `text` (FNV-1a, 64 bits), to notice when it changes.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.split_whitespace().flat_map(str::bytes) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// What `text` (the body of robots.txt; `None` when there is none) says to
/// us about `endpoint`.
pub fn report(text: Option<&str>, endpoint: Option<&url::Url>) -> RobotsReport {
    let Some(text) = text else {
        return RobotsReport {
            host: endpoint.and_then(crate::job_boards::host_of),
            found: false,
            endpoint_allowed: endpoint.map(|_| true),
            ..RobotsReport::default()
        };
    };
    let (rules, crawl_delay) = rules_for_us(text);
    let (endpoint_allowed, deciding_rule) = match endpoint {
        Some(url) => {
            let (ok, rule) = allowed(&rules, &path_of(url));
            (Some(ok), rule)
        }
        None => (None, None),
    };
    RobotsReport {
        host: endpoint.and_then(crate::job_boards::host_of),
        found: true,
        rules,
        crawl_delay,
        endpoint_allowed,
        deciding_rule,
        fingerprint: fingerprint(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMOTIVE: &str = "
        User-agent: *
        Sitemap:https://remotive.example/sitemap.xml
        Disallow: /jobs/*
        Disallow: /api/*
        Disallow: /*category=
    ";

    #[test]
    fn the_star_group_decides_for_us_by_the_longest_rule() {
        let (rules, _) = rules_for_us(REMOTIVE);
        assert_eq!(rules.len(), 3);
        let (ok, rule) = allowed(&rules, "/api/remote-jobs?category=software-dev");
        assert!(!ok);
        assert!(rule.unwrap().starts_with("Disallow"));
        assert!(allowed(&rules, "/remote-jobs").0);
        let robots = "User-agent: *\nDisallow: /\nAllow: /jobs/api\n\nUser-agent: Other\nAllow: /";
        let (rules, _) = rules_for_us(robots);
        assert!(allowed(&rules, "/jobs/api?cursor=x").0, "Allow is longer");
        assert!(!allowed(&rules, "/admin").0);
    }

    #[test]
    fn our_own_group_wins_and_the_delay_is_read() {
        let robots = "User-agent: *\nCrawl-delay: 1\nAllow: /\n\n\
                      User-agent: AhrefsBot\nDisallow: /\n\n\
                      User-agent: applicamp\nUser-agent: other\nDisallow: /private\nCrawl-delay: 2";
        let (rules, delay) = rules_for_us(robots);
        assert_eq!(delay, Some(2.0));
        assert!(!allowed(&rules, "/private/x").0);
        assert!(allowed(&rules, "/api").0);
        let (star, delay) = rules_for_us("User-agent: *\nCrawl-delay: 1\nAllow: /");
        assert_eq!(delay, Some(1.0));
        assert!(allowed(&star, "/api").0);
    }

    #[test]
    fn wildcards_and_anchors() {
        assert!(matches("/*?page=", "/jobs?page=2"));
        assert!(!matches("/*?page=", "/jobs/api?cursor=x"));
        assert!(matches("/feed$", "/feed"));
        assert!(!matches("/feed$", "/feed/x"));
        assert!(matches("/*.json$", "/a/b.json"));
        assert!(!matches("/api/", "/apiary"));
        // An empty Disallow allows everything.
        let (rules, _) = rules_for_us("User-agent: *\nDisallow:\n");
        assert!(rules.is_empty());
        assert!(allowed(&rules, "/anything").0);
    }

    #[test]
    fn a_report_says_whether_the_endpoint_is_allowed() {
        let url: url::Url = "https://remotive.example/api/remote-jobs?category=software-dev"
            .parse()
            .unwrap();
        let report = report(Some(REMOTIVE), Some(&url));
        assert_eq!(report.endpoint_allowed, Some(false));
        assert_eq!(
            report.deciding_rule.as_deref(),
            Some("Disallow: /*category=")
        );
        assert!(report.found);
        let none = super::report(None, Some(&url));
        assert_eq!(none.endpoint_allowed, Some(true), "no robots.txt allows");
        assert_ne!(fingerprint("a b"), fingerprint("a c"));
        assert_eq!(fingerprint("a  b"), fingerprint("a b"));
    }
}
