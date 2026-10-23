use crate::dns_switcher::util::{get_lease_status, CUSTOM, DEFAULT};
use crate::mikrotik::interactor::MikrotikInteractor;
use std::net::IpAddr;
use std::sync::Arc;

#[derive(Clone)]
pub struct Deps {
    pub mikrotik: Arc<MikrotikInteractor>,
    pub comment: String,
    pub custom_dns: IpAddr,
}

fn ip_to_hex(dns: IpAddr) -> String {
    match dns {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("0x{:02x}{:02x}{:02x}{:02x}", o[0], o[1], o[2], o[3])
        }
        IpAddr::V6(v6) => {
            let o = v6.octets();
            let mut s = String::from("0x");
            for b in o {
                s.push_str(&format!("{:02x}", b));
            }
            s
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Lease exists but is not managed by this app")]
    Unmanaged,
    #[error("Other error")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub async fn setup_dhcp_lease_with_custom_dns_for_client(deps: Deps, ip: String) -> Result<serde_json::Value, Error> {
    let lease = deps.mikrotik.get_dhcp_lease_by_ip(&ip).await.map_err(Error::Other)?;
    let status = get_lease_status(lease.as_ref(), &deps.comment);
    if status == CUSTOM {
        return Ok(serde_json::json!({"ip": ip, "status": CUSTOM}));
    }
    if status != DEFAULT {
        return Err(Error::Unmanaged);
    }
    if lease.is_some() && lease.as_ref().unwrap().get("dynamic").map(|s| s.as_str()) == Some("true") {
        // delete it
        if let Some(l) = &lease {
            if let Some(id) = l.get(".id").or_else(|| l.get("id")) {
                let _ = deps.mikrotik.remove_dhcp_lease(id).await;
            }
        }
    }
    let mac = deps.mikrotik.find_mac_address_by_ip(&ip).await.map_err(Error::Other)?
        .ok_or_else(|| Error::Other("No MAC found".into()))?;
    let opt_name = "Custom DNS Server";
    let mut dhcp_option = deps.mikrotik.get_dhcp_option_by_name(opt_name).await.map_err(Error::Other)?;
    if dhcp_option.is_none() {
        let value = ip_to_hex(deps.custom_dns);
        let created = deps.mikrotik.create_dhcp_option(opt_name, 6, &value).await.map_err(Error::Other)?;
        dhcp_option = Some(created);
    }
    let dhcp_option_name = dhcp_option.as_ref().and_then(|o| o.get("name")).cloned().unwrap_or(opt_name.to_string());
    // For RouterOS, we can pass the name
    let _ = deps.mikrotik.create_dhcp_lease(&ip, &mac, &dhcp_option_name, &deps.comment).await.map_err(Error::Other)?;
    Ok(serde_json::json!({"ip": ip, "status": CUSTOM}))
}
