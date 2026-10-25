//! The addresses the switcher may manage, e.g. the router's DHCP pool. Anything
//! outside it (the router, fixed devices, a server's other addresses) is refused
//! before the router is contacted.

use std::net::{IpAddr, Ipv4Addr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressRange {
    from: Ipv4Addr,
    to: Ipv4Addr,
}

impl AddressRange {
    /// `192.168.88.20-192.168.88.99`, ends included.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let invalid = || format!("invalid address range: {spec:?} (expected A.B.C.D-A.B.C.D)");
        let (from, to) = spec.split_once('-').ok_or_else(invalid)?;
        let from: Ipv4Addr = from.trim().parse().map_err(|_| invalid())?;
        let to: Ipv4Addr = to.trim().parse().map_err(|_| invalid())?;
        if from > to {
            return Err(invalid());
        }
        Ok(AddressRange { from, to })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ip) => self.from <= ip && ip <= self.to,
            IpAddr::V6(_) => false,
        }
    }
}

/// No range configured (local development) allows every address.
pub fn is_allowed(ip: IpAddr, range: Option<&AddressRange>) -> bool {
    range.is_none_or(|r| r.contains(ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn parses_an_inclusive_range() {
        let r = AddressRange::parse("192.168.88.20-192.168.88.99").unwrap();
        assert!(r.contains(ip("192.168.88.20")));
        assert!(r.contains(ip("192.168.88.99")));
    }

    #[test]
    fn rejects_malformed_or_reversed_ranges() {
        for bad in [
            "",
            "192.168.88.20",
            "192.168.88.99-192.168.88.20",
            "192.168.88.20-192.168.88.300",
            "192.168.88.20-192.168.88.30-192.168.88.40",
            "::1-::2",
        ] {
            assert!(AddressRange::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn refuses_addresses_outside_the_range() {
        let r = AddressRange::parse("192.168.88.20-192.168.88.99").unwrap();
        for out in ["192.168.88.19", "192.168.88.100", "172.19.0.1", "::1"] {
            assert!(!is_allowed(ip(out), Some(&r)), "{out}");
        }
    }

    #[test]
    fn no_range_allows_everything() {
        assert!(is_allowed(ip("10.1.2.3"), None));
    }
}
