use mikrotik_rs::MikrotikDevice;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Method, Server};
use tokio::runtime::Runtime;

mod config;
mod dns_service;
mod handlers;
mod utils;

use config::Config;
use handlers::{
    handle_get_dns, handle_not_found, handle_root, handle_set_custom_dns, handle_set_default_dns,
    handle_ui,
};
use utils::get_custom_ip_from_headers;

fn main() {
    // Load configuration from environment
    let config = Config::from_env().expect("Failed to load configuration");

    println!(
        "Starting MikroTik DNS Switcher on http://{}:{}",
        config.host, config.port
    );

    println!("Available endpoints:");
    println!("  GET  /            - Web interface for DNS management");
    println!("  GET  /api         - API information");
    println!("  GET  /api/dns     - Show current DNS server");
    println!(
        "  POST /api/dns/custom - Switch to custom DNS ({})",
        config.custom_dns
    );
    println!("  POST /api/dns/default - Remove custom DNS (use default from DHCP server)");

    // Create a runtime for establishing the initial connection
    let rt = Runtime::new().expect("Failed to create runtime");

    // Establish connection to MikroTik router on startup
    let device = rt.block_on(async {
        let addr = format!("{}:{}", config.mikrotik_host, config.mikrotik_port);
        println!("Establishing connection to MikroTik router at {}", addr);

        MikrotikDevice::connect(
            &addr,
            &config.mikrotik_username,
            Some(&config.mikrotik_password),
        )
        .await
        .expect("Failed to connect to MikroTik router on startup")
    });

    println!("Successfully connected to MikroTik router");

    // Wrap the device in Arc<Mutex<>> for thread-safe sharing
    let shared_device = Arc::new(Mutex::new(device));
    let shared_config = Arc::new(config.clone());

    let server = Server::http(format!("{}:{}", config.host, config.port)).unwrap();
    println!("Server running at http://{}:{}", config.host, config.port);

    for request in server.incoming_requests() {
        let device_clone = shared_device.clone();
        let config_clone = shared_config.clone();
        thread::spawn(move || {
            let method = request.method();
            let url = request.url();

            // Check for X-Real-IP header for reverse proxy setups
            let client_ip = if let Some(custom_ip) = get_custom_ip_from_headers(request.headers()) {
                println!("Using IP from header: {}", custom_ip);
                custom_ip
            } else {
                match request.remote_addr() {
                    Some(addr) => addr.ip().to_string(),
                    None => "unknown".to_string(),
                }
            };

            println!("{} {} from {}", method, url, client_ip);

            let response = match (method, url) {
                (&Method::Get, "/api/dns") => handle_get_dns(&client_ip, device_clone, config_clone.clone()),
                (&Method::Post, "/api/dns/custom") => handle_set_custom_dns(&client_ip, device_clone, config_clone.clone()),
                (&Method::Post, "/api/dns/default") => handle_set_default_dns(&client_ip, device_clone, config_clone.clone()),
                (&Method::Get, "/api") => handle_root(&config_clone),
                (&Method::Get, path) if path.starts_with("/ui") => handle_ui(&config_clone),
                (&Method::Get, "/") => handle_ui(&config_clone),
                _ => handle_not_found(),
            };

            let _ = request.respond(response);
        });
    }
}
