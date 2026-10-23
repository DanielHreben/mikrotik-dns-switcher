use crate::dns_switcher::util::{get_lease_status, CUSTOM, DEFAULT};
use crate::mikrotik::interactor::MikrotikInteractor;
use std::sync::Arc;

#[derive(Clone)]
pub struct Deps {
    pub mikrotik: Arc<MikrotikInteractor>,
    pub comment: String,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Other error")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub async fn remove_custom_dhcp_lease_for_client(deps: Deps, ip: String) -> Result<serde_json::Value, Error> {
    let lease = deps.mikrotik.get_dhcp_lease_by_ip(&ip).await.map_err(Error::Other)?;
    let status = get_lease_status(lease.as_ref(), &deps.comment);
    if status != CUSTOM {
        return Ok(serde_json::json!({"ip": ip, "status": DEFAULT}));
    }
    if let Some(l) = lease {
        if let Some(id) = l.get(".id").or_else(|| l.get("id")) {
            let _ = deps.mikrotik.remove_dhcp_lease(id).await.map_err(Error::Other)?;
        }
    }
    Ok(serde_json::json!({"ip": ip, "status": DEFAULT}))
}
