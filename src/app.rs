//! The HTTP side: `/api/dns`, `/health` and the embedded front end.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::client_ip::{self, Trust};
use crate::range::{AddressRange, is_allowed};
use crate::routeros::Endpoint;
use crate::service::{self, Settings};

pub struct AppState {
    pub endpoint: Endpoint,
    pub settings: Settings,
    pub trust: Trust,
    pub allowed_range: Option<AddressRange>,
    /// One router operation at a time: two clicks on the same device must not both
    /// see "no lease" and both create one.
    pub router_lock: Mutex<()>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/dns", get(get_dns).put(put_dns).delete(delete_dns))
        .route("/health", get(health))
        .fallback(static_file)
        .with_state(state)
}

fn failure(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "ok": false, "error": { "code": code, "message": message } })),
    )
        .into_response()
}

fn not_allowed() -> Response {
    failure(
        StatusCode::FORBIDDEN,
        "ADDRESS_NOT_ALLOWED",
        "This address is not managed by the DNS switcher",
    )
}

fn unexpected() -> Response {
    failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "UNEXPECTED_ERROR",
        "Something went wrong",
    )
}

#[derive(Clone, Copy)]
enum Action {
    Check,
    Enable,
    Disable,
}

async fn handle(
    state: Arc<AppState>,
    remote: SocketAddr,
    headers: HeaderMap,
    action: Action,
) -> Response {
    let header = headers.get("x-real-ip").and_then(|v| v.to_str().ok());
    let Ok(ip) = client_ip::resolve(remote.ip(), header, state.trust) else {
        return not_allowed();
    };
    if !is_allowed(ip, state.allowed_range.as_ref()) {
        info!(%ip, "address outside the managed range");
        return not_allowed();
    }

    let _one_at_a_time = state.router_lock.lock().await;
    let result = async {
        let mut session = state.endpoint.connect().await?;
        match action {
            Action::Check => service::check(&mut session, &state.settings, ip).await,
            Action::Enable => service::enable(&mut session, &state.settings, ip).await,
            Action::Disable => service::disable(&mut session, &state.settings, ip).await,
        }
    }
    .await;

    match result {
        Ok(status) => {
            info!(%ip, status = status.as_str(), "ok");
            Json(json!({ "ok": true, "data": { "ip": ip.to_string(), "status": status.as_str() } }))
                .into_response()
        }
        Err(e) => {
            error!(%ip, error = %e, "request failed");
            unexpected()
        }
    }
}

macro_rules! dns_handler {
    ($name:ident, $action:expr) => {
        async fn $name(
            State(state): State<Arc<AppState>>,
            ConnectInfo(remote): ConnectInfo<SocketAddr>,
            headers: HeaderMap,
        ) -> Response {
            handle(state, remote, headers, $action).await
        }
    };
}
dns_handler!(get_dns, Action::Check);
dns_handler!(put_dns, Action::Enable);
dns_handler!(delete_dns, Action::Disable);

async fn health() -> Json<Value> {
    let timestamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    Json(json!({ "ok": true, "data": { "timestamp": timestamp } }))
}

const ASSETS: &[(&str, &str, &[u8])] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../public/index.html"),
    ),
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_bytes!("../public/index.html"),
    ),
    (
        "/app.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../public/app.js"),
    ),
    (
        "/styles.css",
        "text/css; charset=utf-8",
        include_bytes!("../public/styles.css"),
    ),
];

async fn static_file(uri: Uri) -> Response {
    match ASSETS.iter().find(|(path, ..)| *path == uri.path()) {
        Some((_, content_type, body)) => (
            [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
            *body,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
