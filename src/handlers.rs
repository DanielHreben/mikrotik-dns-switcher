use crate::config::Config;
use crate::dns_service::{get_current_dns, remove_custom_dns, set_dns};
use crate::utils::json_response;
use mikrotik_rs::MikrotikDevice;
use std::sync::{Arc, Mutex};
use tiny_http::Response;

/// Handle GET /api - Show API information and available endpoints
pub fn handle_root(config: &Config) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = format!(r#"{{
  "service": "MikroTik DNS Switcher",
  "version": "1.0.0",
  "ui": "Visit / for the web interface",
  "endpoints": {{
    "GET /api/dns": "Show current DNS server",
    "POST /api/dns/custom": "Switch to custom DNS ({})",
    "POST /api/dns/default": "Remove custom DNS (use default from DHCP server)"
  }}
}}"#, config.custom_dns);

    json_response(&body, 200)
}

/// Handle GET /dns - Show current DNS for the client
pub fn handle_get_dns(
    client_ip: &str,
    device: Arc<Mutex<MikrotikDevice>>,
    config: Arc<Config>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    println!("GET /dns endpoint called for client: {}", client_ip);
    match get_current_dns(client_ip, device, config) {
        Ok(dns) => {
            println!("Successfully retrieved DNS for {}: {}", client_ip, dns);
            let body = format!(
                r#"{{"client_ip": "{}", "current_dns": "{}"}}"#,
                client_ip, dns
            );
            json_response(&body, 200)
        }
        Err(err) => {
            println!("Error getting DNS for {}: {}", client_ip, err);
            let body = format!(
                r#"{{"client_ip": "{}", "error": "Failed to get DNS: {}"}}"#,
                client_ip, err
            );
            json_response(&body, 500)
        }
    }
}

/// Handle POST /dns/custom - Set custom DNS for the client
pub fn handle_set_custom_dns(
    client_ip: &str,
    device: Arc<Mutex<MikrotikDevice>>,
    config: Arc<Config>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    println!("POST /dns/custom endpoint called for client: {}", client_ip);
    match set_dns(client_ip, &config.custom_dns, device, config.clone()) {
        Ok(()) => {
            let body = format!(
                r#"{{"client_ip": "{}", "message": "DNS changed to custom", "dns": "{}"}}"#,
                client_ip, config.custom_dns
            );
            json_response(&body, 200)
        }
        Err(err) => {
            let body = format!(
                r#"{{"client_ip": "{}", "error": "Failed to set custom DNS: {}"}}"#,
                client_ip, err
            );
            json_response(&body, 500)
        }
    }
}

/// Handle POST /dns/default - Remove custom DNS for the client (revert to default)
pub fn handle_set_default_dns(
    client_ip: &str,
    device: Arc<Mutex<MikrotikDevice>>,
    config: Arc<Config>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    println!(
        "POST /dns/default endpoint called for client: {}",
        client_ip
    );
    match remove_custom_dns(client_ip, device, config) {
        Ok(()) => {
            let body = format!(
                r#"{{"client_ip": "{}", "message": "Custom DNS removed, using default from DHCP server"}}"#,
                client_ip
            );
            json_response(&body, 200)
        }
        Err(err) => {
            let body = format!(
                r#"{{"client_ip": "{}", "error": "Failed to remove custom DNS: {}"}}"#,
                client_ip, err
            );
            json_response(&body, 500)
        }
    }
}

/// Handle 404 Not Found
pub fn handle_not_found() -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(r#"{"error": "Not found"}"#, 404)
}

/// Handle GET /ui - Serve the HTML UI
pub fn handle_ui(config: &Config) -> Response<std::io::Cursor<Vec<u8>>> {
    // Read HTML template from file
    let html_template = match std::fs::read_to_string("static/index.html") {
        Ok(content) => content,
        Err(_) => {
            // Fallback if file doesn't exist
            return json_response(r#"{"error": "UI template not found"}"#, 500);
        }
    };
    
    // Replace placeholder with actual custom DNS value
    let html = html_template.replace("{{CUSTOM_DNS}}", &config.custom_dns);

    let mut response = Response::from_string(html);
    
    // Add HTML content type header
    if let Ok(header) = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        response = response.with_header(header);
    }
    
    response
}
