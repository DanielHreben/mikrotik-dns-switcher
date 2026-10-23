use crate::mikrotik::client::RouterosClient;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct MikrotikInteractor {
    client: Arc<RouterosClient>,
}

impl MikrotikInteractor {
    pub fn new(client: RouterosClient) -> Self {
        MikrotikInteractor {
            client: Arc::new(client),
        }
    }

    pub async fn get_dhcp_lease_by_ip(&self, ip: &str) -> Result<Option<HashMap<String, String>>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&["/ip/dhcp-server/lease/print", &format!("?address={}", ip)]).await?;
        if res.is_empty() {
            Ok(None)
        } else {
            Ok(Some(res[0].clone()))
        }
    }

    pub async fn create_dhcp_lease(&self, address: &str, mac_address: &str, dhcp_option: &str, comment: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&[
            "/ip/dhcp-server/lease/add",
            &format!("=address={}", address),
            &format!("=mac-address={}", mac_address),
            &format!("=dhcp-option={}", dhcp_option),
            &format!("=comment={}", comment),
        ]).await?;
        // Try to get the created lease
        if let Some(r) = res.first() {
            if let Some(id) = r.get("ret") {
                // Fetch it
                let leases = self.client.write(&["/ip/dhcp-server/lease/print"]).await?;
                for l in leases {
                    if l.get(".id") == Some(id) || l.get("id") == Some(id) {
                        return Ok(l);
                    }
                }
            }
        }
        // Return the result we have
        Ok(res.into_iter().next().unwrap_or_default())
    }

    pub async fn find_mac_address_by_ip(&self, ip: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&["/ip/arp/print", &format!("?address={}", ip)]).await?;
        if res.is_empty() {
            Ok(None)
        } else {
            Ok(res[0].get("mac-address").cloned().or_else(|| res[0].get("macAddress").cloned()))
        }
    }

    pub async fn remove_dhcp_lease(&self, id: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&["/ip/dhcp-server/lease/remove", &format!("=numbers={}", id)]).await?;
        Ok(res.into_iter().next().unwrap_or_default())
    }

    pub async fn get_dhcp_option_by_name(&self, name: &str) -> Result<Option<HashMap<String, String>>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&["/ip/dhcp-server/option/print", &format!("?name={}", name)]).await?;
        if res.is_empty() {
            Ok(None)
        } else {
            Ok(Some(res[0].clone()))
        }
    }

    pub async fn create_dhcp_option(&self, name: &str, code: u32, value: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
        let res = self.client.write(&[
            "/ip/dhcp-server/option/add",
            &format!("=name={}", name),
            &format!("=code={}", code),
            &format!("=value={}", value),
            "=comment=DNS-Switcher-Managed",
        ]).await?;
        // Try to get it
        if let Some(r) = res.first() {
            if let Some(id) = r.get("ret") {
                let opts = self.client.write(&["/ip/dhcp-server/option/print", &format!("?name={}", name)]).await?;
                for o in opts {
                    if o.get(".id") == Some(id) || o.get("id") == Some(id) {
                        return Ok(o);
                    }
                }
            }
        }
        Ok(res.into_iter().next().unwrap_or_default())
    }
}
