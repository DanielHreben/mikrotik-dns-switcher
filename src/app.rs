//! The HTTP side: `/api/dns`, `/health` and the embedded front end.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::{Next, from_fn_with_state, map_response};
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
    /// Host names the service answers to; empty means any. Stops a web page from
    /// reaching the service under a name it controls (DNS rebinding).
    pub allowed_hosts: Vec<String>,
    /// How long a request waits for its turn at the router before giving up.
    pub lock_wait: Duration,
    /// One router operation at a time: two clicks on the same device must not both
    /// see "no lease" and both create one.
    pub router_lock: Mutex<()>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/dns", get(get_dns).put(put_dns).delete(delete_dns))
        .route("/health", get(health))
        .fallback(static_file)
        .layer(from_fn_with_state(state.clone(), check_host))
        .layer(map_response(security_headers))
        .with_state(state)
}

/// `Host` without its port, lower case.
fn host_name(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default().to_string(),
        None => host.split(':').next().unwrap_or_default().to_string(),
    }
}

async fn check_host(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    // The container health check asks for "localhost".
    if !state.allowed_hosts.is_empty() && req.uri().path() != "/health" {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(host_name);
        if !host.is_some_and(|h| state.allowed_hosts.contains(&h)) {
            return failure(
                StatusCode::FORBIDDEN,
                "HOST_NOT_ALLOWED",
                "This name is not served by the DNS switcher",
            );
        }
    }
    next.run(req).await
}

/// The page is never meant to be framed or embedded, and answers about one device
/// must not be cached.
async fn security_headers(mut res: Response) -> Response {
    let headers = res.headers_mut();
    let json = headers
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"));
    for (name, value) in [
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    if json {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    res
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

    let Ok(_one_at_a_time) = tokio::time::timeout(state.lock_wait, state.router_lock.lock()).await
    else {
        error!(%ip, "gave up waiting for the router");
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "BUSY",
            "The router is busy, try again in a moment",
        );
    };
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
        Err(e @ service::Error::LimitReached(_)) => {
            error!(%ip, error = %e, "request refused");
            failure(
                StatusCode::TOO_MANY_REQUESTS,
                "LIMIT_REACHED",
                "Too many devices already use custom DNS",
            )
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
