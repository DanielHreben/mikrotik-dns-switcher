//! The router operations the switcher needs, over a [`Session`].

use std::net::{IpAddr, Ipv4Addr};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::routeros::{Error, Row, Session};

pub struct Router<'a, S> {
    session: &'a mut Session<S>,
}

pub const OPTION_NAME: &str = "Custom DNS Server";
/// DHCP option code for DNS servers.
pub const DNS_OPTION_CODE: u8 = 6;

/// `192.168.88.113` as RouterOS writes a raw option value: `0xc0a85871`.
pub fn dns_option_value(dns: Ipv4Addr) -> String {
    let o = dns.octets();
    format!("0x{:02x}{:02x}{:02x}{:02x}", o[0], o[1], o[2], o[3])
}

impl<'a, S: AsyncRead + AsyncWrite + Unpin> Router<'a, S> {
    pub fn new(session: &'a mut Session<S>) -> Self {
        Router { session }
    }

    async fn first(&mut self, words: &[&str]) -> Result<Option<Row>, Error> {
        Ok(self.session.run(words).await?.rows.into_iter().next())
    }

    pub async fn lease_by_ip(&mut self, ip: IpAddr) -> Result<Option<Row>, Error> {
        self.first(&["/ip/dhcp-server/lease/print", &format!("?address={ip}")])
            .await
    }

    pub async fn mac_by_ip(&mut self, ip: IpAddr) -> Result<Option<String>, Error> {
        let arp = self
            .first(&["/ip/arp/print", &format!("?address={ip}")])
            .await?;
        Ok(arp.and_then(|mut row| row.remove("mac-address")))
    }

    pub async fn option_by_name(&mut self, name: &str) -> Result<Option<Row>, Error> {
        self.first(&["/ip/dhcp-server/option/print", &format!("?name={name}")])
            .await
    }

    /// Returns the new option's id.
    pub async fn create_option(&mut self, value: &str, comment: &str) -> Result<String, Error> {
        let reply = self
            .session
            .run(&[
                "/ip/dhcp-server/option/add",
                &format!("=name={OPTION_NAME}"),
                &format!("=code={DNS_OPTION_CODE}"),
                &format!("=value={value}"),
                &format!("=comment={comment}"),
            ])
            .await?;
        reply
            .ret
            .ok_or_else(|| Error::Protocol("option/add returned no id".into()))
    }

    /// How many leases carry the app's comment.
    pub async fn managed_lease_count(&mut self, comment: &str) -> Result<usize, Error> {
        let reply = self
            .session
            .run(&[
                "/ip/dhcp-server/lease/print",
                &format!("?comment={comment}"),
                "=.proplist=.id",
            ])
            .await?;
        Ok(reply.rows.len())
    }

    pub async fn set_option_value(&mut self, id: &str, value: &str) -> Result<(), Error> {
        self.session
            .run(&[
                "/ip/dhcp-server/option/set",
                &format!("=.id={id}"),
                &format!("=value={value}"),
            ])
            .await
            .map(drop)
    }

    pub async fn create_lease(
        &mut self,
        ip: IpAddr,
        mac: &str,
        option_id: &str,
        comment: &str,
    ) -> Result<(), Error> {
        self.session
            .run(&[
                "/ip/dhcp-server/lease/add",
                &format!("=address={ip}"),
                &format!("=mac-address={mac}"),
                &format!("=dhcp-option={option_id}"),
                &format!("=comment={comment}"),
            ])
            .await
            .map(drop)
    }

    pub async fn remove_lease(&mut self, id: &str) -> Result<(), Error> {
        self.session
            .run(&["/ip/dhcp-server/lease/remove", &format!("=.id={id}")])
            .await
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_option_is_the_address_in_hex() {
        assert_eq!(
            dns_option_value(Ipv4Addr::new(192, 168, 88, 113)),
            "0xc0a85871"
        );
        assert_eq!(dns_option_value(Ipv4Addr::new(8, 8, 8, 8)), "0x08080808");
    }
}
