//! Reaching job boards the registry names, and nothing private.
//!
//! The registry's addresses come from a config the owner approved, a check
//! proposed from a board's own pages, or a link someone suggested: never
//! trusted to point at the public web. Every such request goes through a
//! client whose DNS resolver drops private, loopback, link-local and other
//! non-public addresses (so a name that resolves to one, now or after a
//! rebind, is never reached), and an address written as an IP is checked
//! the same way before it is asked. Only `https` on its default port,
//! unless the instance allows private hosts (tests, a laptop demo with
//! fake boards: `APPLICAMP_SOURCES__ALLOW_PRIVATE_HOSTS`).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::{Host, Url};

/// Whether `ip` is on the public internet: not private, loopback,
/// link-local (cloud metadata), shared (CGNAT), documentation,
/// benchmarking, multicast, broadcast, reserved or unspecified.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique local fc00::/7, link-local fe80::/10.
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                // Documentation 2001:db8::/32, NAT64 64:ff9b::/96.
                || (first == 0x2001 && v6.segments()[1] == 0x0db8)
                || (first == 0x0064 && v6.segments()[1] == 0xff9b))
        }
    }
}

fn is_public_v4(v4: Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    !(v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || a == 0
        // Shared address space 100.64.0.0/10.
        || (a == 100 && (64..128).contains(&b))
        // IETF protocol assignments 192.0.0.0/24, benchmarking 198.18.0.0/15.
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b))
        // Reserved 240.0.0.0/4.
        || a >= 240)
}

/// Why an address is not asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Not `https` (or not http(s) at all), or not on its default port.
    Scheme,
    /// An IP address, or a name like `localhost`, that is not public.
    NotPublic,
}

/// Whether `url` may be asked: `https` on its default port at a host that
/// is not a private IP or `localhost`, unless `allow_private` (then any
/// http(s) address).
pub fn check(url: &Url, allow_private: bool) -> Result<(), Refused> {
    if allow_private {
        return if matches!(url.scheme(), "https" | "http") && url.host().is_some() {
            Ok(())
        } else {
            Err(Refused::Scheme)
        };
    }
    if url.scheme() != "https" || url.port().is_some_and(|p| p != 443) {
        return Err(Refused::Scheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Refused::Scheme);
    }
    match url.host() {
        Some(Host::Ipv4(ip)) if is_public(IpAddr::V4(ip)) => Ok(()),
        Some(Host::Ipv6(ip)) if is_public(IpAddr::V6(ip)) => Ok(()),
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            let local = name == "localhost"
                || name.ends_with(".localhost")
                || name.ends_with(".local")
                || name.ends_with(".internal")
                || !name.contains('.');
            if local {
                Err(Refused::NotPublic)
            } else {
                Ok(())
            }
        }
        _ => Err(Refused::NotPublic),
    }
}

/// A DNS resolver that answers only public addresses: a name with none is
/// an error, so the request never leaves.
#[derive(Debug, Clone, Copy, Default)]
pub struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            type BoxError = Box<dyn std::error::Error + Send + Sync>;
            let found: Vec<SocketAddr> = match tokio::net::lookup_host((host.as_str(), 0)).await {
                Ok(addrs) => addrs.filter(|addr| is_public(addr.ip())).collect(),
                Err(e) => return Err::<Addrs, BoxError>(Box::new(e)),
            };
            if found.is_empty() {
                return Err::<Addrs, BoxError>(format!("{host} has no public address").into());
            }
            let addrs: Addrs = Box::new(found.into_iter());
            Ok::<Addrs, BoxError>(addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(raw: &str) -> IpAddr {
        raw.parse().unwrap()
    }

    #[test]
    fn only_public_addresses_are_public() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!is_public(ip(private)), "{private}");
        }
        for public in ["1.1.1.1", "151.101.1.1", "2606:4700::1111"] {
            assert!(is_public(ip(public)), "{public}");
        }
    }

    #[test]
    fn only_https_to_public_hosts() {
        let ok = |raw: &str| check(&raw.parse().unwrap(), false);
        assert_eq!(ok("https://himalayas.app/jobs/api"), Ok(()));
        assert_eq!(ok("http://himalayas.app/jobs/api"), Err(Refused::Scheme));
        assert_eq!(ok("https://himalayas.app:8443/x"), Err(Refused::Scheme));
        assert_eq!(ok("https://user:pw@himalayas.app/x"), Err(Refused::Scheme));
        assert_eq!(ok("https://127.0.0.1/x"), Err(Refused::NotPublic));
        assert_eq!(ok("https://[::1]/x"), Err(Refused::NotPublic));
        assert_eq!(ok("https://localhost/x"), Err(Refused::NotPublic));
        assert_eq!(ok("https://metadata.internal/x"), Err(Refused::NotPublic));
        assert_eq!(ok("https://fake-jobs/x"), Err(Refused::NotPublic));
        let dev = check(&"http://fake-jobs:9020/x".parse().unwrap(), true);
        assert_eq!(dev, Ok(()), "a demo with fake boards allows private hosts");
    }
}
