use std::net::IpAddr;
use std::str::FromStr;
use crate::allowed_range::AllowedRange;

#[derive(Debug, Clone)]
pub struct Config {
    pub mikrotik_host: String,
    pub mikrotik_username: String,
    pub mikrotik_password: String,
    pub mikrotik_tls_ca: Option<String>,
    pub port: u16,
    pub custom_dns: IpAddr,
    pub comment: String,
    pub trusted_proxy: Option<IpAddr>,
    pub allow_target_ip: bool,
    pub allowed_range: Option<AllowedRange>,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let mikrotik_host = std::env::var("MIKROTIK_HOST").expect("MIKROTIK_HOST required");
        let mikrotik_username = std::env::var("MIKROTIK_USERNAME").expect("MIKROTIK_USERNAME required");
        let mikrotik_password = std::env::var("MIKROTIK_PASSWORD").expect("MIKROTIK_PASSWORD required");
        let mikrotik_tls_ca = std::env::var("MIKROTIK_TLS_CA").ok();
        let port = std::env::var("PORT").unwrap_or("3000".to_string()).parse::<u16>()?;
        let custom_dns_str = std::env::var("CUSTOM_DNS").expect("CUSTOM_DNS required");
        let custom_dns = IpAddr::from_str(&custom_dns_str)?;
        let comment = std::env::var("APP_COMMENT").unwrap_or("DNS-Switcher-Managed".to_string());
        let trusted_proxy = std::env::var("TRUSTED_PROXY").ok().and_then(|s| IpAddr::from_str(&s).ok());
        let allow_target_ip = std::env::var("ALLOW_TARGET_IP").unwrap_or("false".to_string()) == "true";
        let allowed_range = std::env::var("ALLOWED_RANGE").ok().and_then(|s| AllowedRange::from_spec(&s));
        Ok(Config {
            mikrotik_host,
            mikrotik_username,
            mikrotik_password,
            mikrotik_tls_ca,
            port,
            custom_dns,
            comment,
            trusted_proxy,
            allow_target_ip,
            allowed_range,
        })
    }
}
