use crate::dns_switcher::util::{get_lease_status, CUSTOM, DEFAULT, UNMANAGED};
use crate::mikrotik::interactor::MikrotikInteractor;
use std::sync::Arc;

#[derive(Clone)]
pub struct Deps {
    pub mikrotik: Arc<MikrotikInteractor>,
    pub comment: String,
}

pub async fn check_if_client_has_custom_dhcp_lease(deps: Deps, ip: String) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = deps.mikrotik.get_dhcp_lease_by_ip(&ip).await?;
    let status = get_lease_status(lease.as_ref(), &deps.comment);
    Ok(serde_json::json!({
        "ip": ip,
        "status": status
    }))
}
