#[derive(Clone)]
pub struct Deps {}

pub async fn check_service_health(_deps: Deps) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::json!({"ok": true}))
}
