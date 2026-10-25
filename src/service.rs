//! What each request does to the router. Mirrors the Node implementation this
//! replaces: a lease with our comment is CUSTOM, a dynamic lease or none is
//! DEFAULT, any other static lease is not ours to touch.

use std::net::{IpAddr, Ipv4Addr};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::router::{DNS_OPTION_CODE, OPTION_NAME, Router, dns_option_value};
use crate::routeros::{Error as RouterError, Row, Session};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Default,
    Custom,
    Unmanaged,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Default => "DEFAULT",
            Status::Custom => "CUSTOM",
            Status::Unmanaged => "UNMANAGED",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Router(#[from] RouterError),
    #[error("lease for {0} is static but not managed by this app")]
    Unmanaged(IpAddr),
    #[error("no DHCP lease and no ARP entry found for {0}")]
    NoMac(IpAddr),
    #[error("lease for {0} has no .id")]
    NoId(IpAddr),
    #[error("ARP entry for {0} does not match the MAC of its DHCP lease")]
    MacMismatch(IpAddr),
    #[error("{0} leases are already managed; the limit is reached")]
    LimitReached(usize),
    #[error("a DHCP option named {OPTION_NAME:?} exists that this app did not create")]
    OptionConflict,
}

/// What the app manages and how, from the configuration.
#[derive(Clone, Debug)]
pub struct Settings {
    pub comment: String,
    pub custom_dns: Ipv4Addr,
    /// How many leases the app may own at once. Every enabled device holds a
    /// permanent static lease, so without a limit one device could use up the pool.
    pub max_managed_leases: usize,
}

fn is_dynamic(lease: &Row) -> bool {
    lease
        .get("dynamic")
        .is_some_and(|d| d == "true" || d == "yes")
}

pub fn status_of(lease: Option<&Row>, comment: &str) -> Status {
    match lease {
        None => Status::Default,
        Some(l) if is_dynamic(l) => Status::Default,
        Some(l) if l.get("comment").map(String::as_str) != Some(comment) => Status::Unmanaged,
        Some(_) => Status::Custom,
    }
}

fn lease_id(lease: &Row, ip: IpAddr) -> Result<&str, Error> {
    lease.get(".id").map(String::as_str).ok_or(Error::NoId(ip))
}

pub async fn check<S: AsyncRead + AsyncWrite + Unpin>(
    session: &mut Session<S>,
    settings: &Settings,
    ip: IpAddr,
) -> Result<Status, Error> {
    let lease = Router::new(session).lease_by_ip(ip).await?;
    Ok(status_of(lease.as_ref(), &settings.comment))
}

/// The id of the "Custom DNS Server" option, created or corrected as needed.
async fn ensure_option<S: AsyncRead + AsyncWrite + Unpin>(
    router: &mut Router<'_, S>,
    settings: &Settings,
    ip: IpAddr,
) -> Result<String, Error> {
    let want = dns_option_value(settings.custom_dns);
    let Some(option) = router.option_by_name(OPTION_NAME).await? else {
        return Ok(router.create_option(&want, &settings.comment).await?);
    };
    let id = option.get(".id").cloned().ok_or(Error::NoId(ip))?;
    let right = option.get("code").map(String::as_str) == Some(&DNS_OPTION_CODE.to_string())
        && option
            .get("value")
            .is_some_and(|v| v.eq_ignore_ascii_case(&want));
    if !right {
        // Our own option with an old address (CUSTOM_DNS changed): correct it. Someone
        // else's option of that name is not ours to edit.
        if option.get("comment") != Some(&settings.comment) {
            return Err(Error::OptionConflict);
        }
        router.set_option_value(&id, &want).await?;
    }
    Ok(id)
}

pub async fn enable<S: AsyncRead + AsyncWrite + Unpin>(
    session: &mut Session<S>,
    settings: &Settings,
    ip: IpAddr,
) -> Result<Status, Error> {
    let mut router = Router::new(session);
    let lease = router.lease_by_ip(ip).await?;
    match status_of(lease.as_ref(), &settings.comment) {
        Status::Custom => return Ok(Status::Custom),
        Status::Unmanaged => return Err(Error::Unmanaged(ip)),
        Status::Default => {}
    }

    // Everything that can refuse comes before the dynamic lease is touched.
    let mac = router.mac_by_ip(ip).await?.ok_or(Error::NoMac(ip))?;
    if let Some(lease) = &lease {
        // The ARP table must agree with the lease the address was handed out under;
        // otherwise a poisoned entry could bind someone else's MAC to this address.
        let lease_mac = lease
            .get("mac-address")
            .or_else(|| lease.get("active-mac-address"));
        if !lease_mac.is_some_and(|m| m.eq_ignore_ascii_case(&mac)) {
            return Err(Error::MacMismatch(ip));
        }
    }
    let managed = router.managed_lease_count(&settings.comment).await?;
    if managed >= settings.max_managed_leases {
        return Err(Error::LimitReached(managed));
    }
    let option_id = ensure_option(&mut router, settings, ip).await?;

    // A dynamic lease is replaced by a static one that carries the DNS option.
    if let Some(lease) = &lease {
        router.remove_lease(lease_id(lease, ip)?).await?;
    }
    router
        .create_lease(ip, &mac, &option_id, &settings.comment)
        .await?;
    Ok(Status::Custom)
}

pub async fn disable<S: AsyncRead + AsyncWrite + Unpin>(
    session: &mut Session<S>,
    settings: &Settings,
    ip: IpAddr,
) -> Result<Status, Error> {
    let mut router = Router::new(session);
    let lease = router.lease_by_ip(ip).await?;
    match (status_of(lease.as_ref(), &settings.comment), &lease) {
        (Status::Unmanaged, _) => Err(Error::Unmanaged(ip)),
        (Status::Custom, Some(lease)) => {
            router.remove_lease(lease_id(lease, ip)?).await?;
            Ok(Status::Default)
        }
        _ => Ok(Status::Default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(pairs: &[(&str, &str)]) -> Row {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn no_lease_is_default() {
        assert_eq!(status_of(None, "c"), Status::Default);
    }

    #[test]
    fn a_dynamic_lease_is_default_whatever_its_comment() {
        assert_eq!(
            status_of(Some(&lease(&[("dynamic", "true")])), "c"),
            Status::Default
        );
        assert_eq!(
            status_of(Some(&lease(&[("dynamic", "true"), ("comment", "x")])), "c"),
            Status::Default
        );
    }

    #[test]
    fn a_static_lease_is_ours_only_with_our_comment() {
        assert_eq!(
            status_of(Some(&lease(&[("dynamic", "false"), ("comment", "c")])), "c"),
            Status::Custom
        );
        assert_eq!(
            status_of(
                Some(&lease(&[("dynamic", "false"), ("comment", "m1mbp")])),
                "c"
            ),
            Status::Unmanaged
        );
        assert_eq!(
            status_of(Some(&lease(&[("dynamic", "false")])), "c"),
            Status::Unmanaged
        );
    }
}
