//! Which device a request acts for: the `X-Real-IP` header when the request comes
//! from the trusted proxy (or target IPs are allowed), otherwise the address the
//! connection comes from.

use std::net::{IpAddr, Ipv4Addr};

#[derive(Clone, Copy, Debug)]
pub struct Trust {
    /// The reverse proxy whose `X-Real-IP` header is trusted (e.g. nginx).
    pub proxy: Option<Ipv4Addr>,
    /// Trust `X-Real-IP` from any caller, so `?ip=` in the page can act for another
    /// device. Local development only.
    pub allow_target_ip: bool,
}

/// An `X-Real-IP` the switcher trusts but cannot read as an address.
#[derive(Debug, PartialEq, Eq)]
pub struct BadHeader;

fn unmap(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

pub fn resolve(remote: IpAddr, header: Option<&str>, trust: Trust) -> Result<IpAddr, BadHeader> {
    let remote = unmap(remote);
    let from_proxy = trust.proxy.is_some_and(|p| remote == IpAddr::V4(p));
    let header = header.and_then(|h| h.split(',').next()).map(str::trim);
    match header {
        Some(h) if !h.is_empty() && (trust.allow_target_ip || from_proxy) => {
            h.parse().map(unmap).map_err(|_| BadHeader)
        }
        _ => Ok(remote),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROXY: Ipv4Addr = Ipv4Addr::new(172, 19, 0, 100);

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn trusting_proxy() -> Trust {
        Trust {
            proxy: Some(PROXY),
            allow_target_ip: false,
        }
    }

    #[test]
    fn uses_the_header_when_the_request_comes_from_the_trusted_proxy() {
        assert_eq!(
            resolve(ip("172.19.0.100"), Some("192.168.88.42"), trusting_proxy()),
            Ok(ip("192.168.88.42"))
        );
    }

    #[test]
    fn accepts_the_proxy_as_an_ipv4_mapped_ipv6_address() {
        assert_eq!(
            resolve(
                ip("::ffff:172.19.0.100"),
                Some("192.168.88.42"),
                trusting_proxy()
            ),
            Ok(ip("192.168.88.42"))
        );
    }

    #[test]
    fn ignores_the_header_from_any_other_caller() {
        assert_eq!(
            resolve(
                ip("::ffff:172.19.0.18"),
                Some("192.168.88.42"),
                trusting_proxy()
            ),
            Ok(ip("172.19.0.18"))
        );
    }

    #[test]
    fn ignores_the_header_when_nothing_is_trusted() {
        let none = Trust {
            proxy: None,
            allow_target_ip: false,
        };
        assert_eq!(
            resolve(ip("192.168.88.7"), Some("192.168.88.42"), none),
            Ok(ip("192.168.88.7"))
        );
    }

    #[test]
    fn target_ips_trust_the_header_from_anyone() {
        let dev = Trust {
            proxy: None,
            allow_target_ip: true,
        };
        assert_eq!(
            resolve(ip("127.0.0.1"), Some("192.168.88.42"), dev),
            Ok(ip("192.168.88.42"))
        );
    }

    #[test]
    fn a_missing_or_blank_header_falls_back_to_the_connection() {
        let proxy = ip("172.19.0.100");
        assert_eq!(resolve(proxy, None, trusting_proxy()), Ok(proxy));
        assert_eq!(resolve(proxy, Some("  "), trusting_proxy()), Ok(proxy));
    }

    #[test]
    fn only_the_first_address_of_a_list_counts() {
        assert_eq!(
            resolve(
                ip("172.19.0.100"),
                Some(" 192.168.88.42, 10.0.0.1"),
                trusting_proxy()
            ),
            Ok(ip("192.168.88.42"))
        );
    }

    #[test]
    fn a_trusted_header_that_is_not_an_address_is_refused() {
        assert_eq!(
            resolve(ip("172.19.0.100"), Some("not-an-ip"), trusting_proxy()),
            Err(BadHeader)
        );
    }
}
