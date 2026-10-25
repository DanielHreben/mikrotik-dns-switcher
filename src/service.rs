//! What each request does to the router. Mirrors the Node implementation this
//! replaces: a lease with our comment is CUSTOM, a dynamic lease or none is
//! DEFAULT, any other static lease is not ours to touch.

use std::net::{IpAddr, Ipv4Addr};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::router::{OPTION_NAME, Router, dns_option_value};
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
}

/// What the app manages and how, from the configuration.
#[derive(Clone, Debug)]
pub struct Settings {
    pub comment: String,
    pub custom_dns: Ipv4Addr,
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

    // A dynamic lease is replaced by a static one that carries the DNS option.
    if let Some(lease) = &lease {
        router.remove_lease(lease_id(lease, ip)?).await?;
    }
    let mac = router.mac_by_ip(ip).await?.ok_or(Error::NoMac(ip))?;
    let option_id = match router.option_by_name(OPTION_NAME).await? {
        Some(option) => option.get(".id").cloned().ok_or(Error::NoId(ip))?,
        None => {
            let value = dns_option_value(settings.custom_dns);
            router.create_option(&value, &settings.comment).await?
        }
    };
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
