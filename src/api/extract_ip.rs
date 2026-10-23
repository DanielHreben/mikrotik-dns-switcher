use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};
use crate::config::Config;

pub fn extract_client_ip(headers: &HeaderMap, addr: Option<SocketAddr>, config: &Config) -> IpAddr {
    // Check trusted proxy
    if let Some(trusted) = config.trusted_proxy {
        if let Some(conn_addr) = addr {
            if conn_addr.ip() == trusted {
                if let Some(h) = headers.get("x-real-ip") {
                    if let Ok(s) = h.to_str() {
                        if let Ok(ip) = s.parse::<IpAddr>() {
                            return ip;
                        }
                    }
                }
            }
        }
    }
    if config.allow_target_ip {
        if let Some(h) = headers.get("x-real-ip") {
            if let Ok(s) = h.to_str() {
                if let Ok(ip) = s.parse::<IpAddr>() {
                    return ip;
                }
            }
        }
        if let Some(conn_addr) = addr {
            return conn_addr.ip();
        }
    }
    if let Some(conn_addr) = addr {
        return conn_addr.ip();
    }
    "127.0.0.1".parse().unwrap()
}
