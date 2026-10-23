mod allowed_range;
mod api;
mod config;
mod dns_switcher;
mod health;
mod logger;
mod mikrotik;

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, put},
    Json, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing::info;

use crate::allowed_range::{is_allowed, AddressNotAllowedError};
use crate::api::extract_ip::extract_client_ip;
use crate::config::Config;
use crate::mikrotik::client::RouterosClient;
use crate::mikrotik::interactor::MikrotikInteractor;

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    mikrotik: Arc<MikrotikInteractor>,
}

#[tokio::main]
async fn main() {
    logger::init();
    let config = Config::from_env().expect("Failed to load config");
    let port = config.port;
    
    // Connect to MikroTik
    let mikrotik_client = RouterosClient::connect(&config.mikrotik_host, 8728, &config.mikrotik_username, &config.mikrotik_password)
        .await
        .expect("Failed to connect to MikroTik");
    let mikrotik = Arc::new(MikrotikInteractor::new(mikrotik_client));
    
    let state = AppState {
        config: Arc::new(config.clone()),
        mikrotik: mikrotik.clone(),
    };
    
    let api_router = Router::new()
        .route("/dns", get(get_dns).put(put_dns).delete(delete_dns))
        .with_state(state.clone());
    
    let health_router = Router::new()
        .route("/health", get(get_health))
        .with_state(state.clone());
    
    let app = Router::new()
        .nest("/api", api_router)
        .merge(health_router)
        .nest_service("/", ServeDir::new("public"));
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("🚀 MikroTik DNS Switcher running on http://localhost:{}", port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

async fn get_health(State(_state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({"ok": true}))
}

async fn get_dns(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let ip = extract_client_ip(&headers, Some(addr), &state.config);
    if !is_allowed(&ip, &state.config.allowed_range) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "ok": false,
            "error": {"code": "ADDRESS_NOT_ALLOWED", "message": "This address is not managed by the DNS switcher"}
        })));
    }
    let deps = dns_switcher::check_for_client::Deps {
        mikrotik: state.mikrotik.clone(),
        comment: state.config.comment.clone(),
    };
    match dns_switcher::check_for_client::check_if_client_has_custom_dhcp_lease(deps, ip.to_string()).await {
        Ok(data) => (StatusCode::OK, Json(serde_json::json!({"ok": true, "data": data}))),
        Err(e) => {
            tracing::error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": {"code": "UNEXPECTED_ERROR", "message": "Something went wrong"}})))
        }
    }
}

async fn put_dns(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let ip = extract_client_ip(&headers, Some(addr), &state.config);
    if !is_allowed(&ip, &state.config.allowed_range) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "ok": false,
            "error": {"code": "ADDRESS_NOT_ALLOWED", "message": "This address is not managed by the DNS switcher"}
        })));
    }
    let deps = dns_switcher::setup_for_client::Deps {
        mikrotik: state.mikrotik.clone(),
        comment: state.config.comment.clone(),
        custom_dns: state.config.custom_dns,
    };
    match dns_switcher::setup_for_client::setup_dhcp_lease_with_custom_dns_for_client(deps, ip.to_string()).await {
        Ok(data) => (StatusCode::OK, Json(serde_json::json!({"ok": true, "data": data}))),
        Err(dns_switcher::setup_for_client::Error::Unmanaged) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": {"code": "UNEXPECTED_ERROR", "message": "Something went wrong"}}))),
        Err(e) => {
            tracing::error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": {"code": "UNEXPECTED_ERROR", "message": "Something went wrong"}})))
        }
    }
}

async fn delete_dns(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let ip = extract_client_ip(&headers, Some(addr), &state.config);
    if !is_allowed(&ip, &state.config.allowed_range) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "ok": false,
            "error": {"code": "ADDRESS_NOT_ALLOWED", "message": "This address is not managed by the DNS switcher"}
        })));
    }
    let deps = dns_switcher::reset_for_client::Deps {
        mikrotik: state.mikrotik.clone(),
        comment: state.config.comment.clone(),
    };
    match dns_switcher::reset_for_client::remove_custom_dhcp_lease_for_client(deps, ip.to_string()).await {
        Ok(data) => (StatusCode::OK, Json(serde_json::json!({"ok": true, "data": data}))),
        Err(e) => {
            tracing::error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": {"code": "UNEXPECTED_ERROR", "message": "Something went wrong"}})))
        }
    }
}
