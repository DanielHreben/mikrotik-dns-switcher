use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use mikrotik_dns_switcher::app::{AppState, router};
use mikrotik_dns_switcher::config::Config;
use mikrotik_dns_switcher::routeros::Endpoint;
use mikrotik_dns_switcher::service::Settings;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{error, info};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = runtime.block_on(async {
        match std::env::args().nth(1).as_deref() {
            None => serve().await,
            Some("healthcheck") => healthcheck().await,
            Some(other) => Err(format!("unknown command {other:?} (try: healthcheck)")),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            error!("{message}");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> Result<(), String> {
    let config = Config::from_env()?;
    let tls_ca = match &config.mikrotik_tls_ca {
        Some(path) => Some(
            std::fs::read(path).map_err(|e| format!("MIKROTIK_TLS_CA {}: {e}", path.display()))?,
        ),
        None => {
            tracing::warn!(
                "MIKROTIK_TLS_CA is not set: using the plain API on 8728 (development only)"
            );
            None
        }
    };
    let endpoint = Endpoint {
        host: config.mikrotik_host,
        username: config.mikrotik_username,
        password: config.mikrotik_password,
        tls_ca,
        port: None,
    };
    endpoint.check().map_err(|e| e.to_string())?;

    let state = Arc::new(AppState {
        endpoint,
        settings: Settings {
            comment: config.comment,
            custom_dns: config.custom_dns,
            max_managed_leases: config.max_managed_leases,
        },
        trust: config.trust,
        allowed_range: config.allowed_range,
        allowed_hosts: config.allowed_hosts,
        lock_wait: Duration::from_secs(15),
        router_lock: Mutex::new(()),
    });
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    info!(
        "MikroTik DNS Switcher running on http://localhost:{}",
        config.port
    );
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .map_err(|e| e.to_string())
}

async fn shutdown() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    info!("shutting down");
}

/// `mikrotik-dns-switcher healthcheck`, for the container HEALTHCHECK: the image
/// has no curl.
async fn healthcheck() -> Result<(), String> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000u16);
    let probe = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await?;
        Ok::<_, std::io::Error>(reply)
    };
    let reply = tokio::time::timeout(std::time::Duration::from_secs(3), probe)
        .await
        .map_err(|_| "health check timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if reply.starts_with(b"HTTP/1.0 200") || reply.starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err("health check failed".into())
    }
}
