use std::net::IpAddr;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct AllowedRange {
    pub start: IpAddr,
    pub end: IpAddr,
}

#[derive(Debug, thiserror::Error)]
#[error("Address {0} not allowed")]
pub struct AddressNotAllowedError(pub IpAddr);

impl AllowedRange {
    pub fn from_spec(spec: &str) -> Option<Self> {
        // Try CIDR
        if let Ok(net) = ipnet::IpNet::from_str(spec) {
            match net {
                ipnet::IpNet::V4(v4) => {
                    let network = v4.network();
                    let broadcast = v4.broadcast();
                    return Some(AllowedRange {
                        start: IpAddr::V4(network),
                        end: IpAddr::V4(broadcast),
                    });
                }
                ipnet::IpNet::V6(v6) => {
                    let network = v6.network();
                    let broadcast = v6.broadcast();
                    return Some(AllowedRange {
                        start: IpAddr::V6(network),
                        end: IpAddr::V6(broadcast),
                    });
                }
            }
        }
        // Try range
        if let Some((start_str, end_str)) = spec.split_once('-') {
            if let Ok(start) = start_str.trim().parse::<IpAddr>() {
                if let Ok(end) = end_str.trim().parse::<IpAddr>() {
                    return Some(AllowedRange { start, end });
                }
            }
        }
        None
    }
}

pub fn is_allowed(addr: &IpAddr, range: &Option<AllowedRange>) -> bool {
    if range.is_none() {
        return true;
    }
    let r = range.as_ref().unwrap();
    if let (IpAddr::V4(a), IpAddr::V4(s), IpAddr::V4(e)) = (addr, &r.start, &r.end) {
        let a_u = u32::from_be_bytes(a.octets());
        let s_u = u32::from_be_bytes(s.octets());
        let e_u = u32::from_be_bytes(e.octets());
        return a_u >= s_u && a_u <= e_u;
    }
    if let (IpAddr::V6(a), IpAddr::V6(s), IpAddr::V6(e)) = (addr, &r.start, &r.end) {
        let a_u = u128::from_be_bytes(a.octets());
        let s_u = u128::from_be_bytes(s.octets());
        let e_u = u128::from_be_bytes(e.octets());
        return a_u >= s_u && a_u <= e_u;
    }
    false
}
