//! Settings from the environment. Strict: a value that is set but wrong stops the
//! service. Silently ignoring a bad `ALLOWED_RANGE` or `TRUSTED_PROXY` would widen
//! what the switcher accepts.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use crate::client_ip::Trust;
use crate::range::AddressRange;

#[derive(Debug)]
pub struct Config {
    pub mikrotik_host: String,
    pub mikrotik_username: String,
    pub mikrotik_password: String,
    /// Router CA certificate (PEM file). Set: API-SSL on 8729, certificate
    /// verified. Unset: plain API on 8728 (local development only).
    pub mikrotik_tls_ca: Option<PathBuf>,
    pub port: u16,
    pub custom_dns: Ipv4Addr,
    pub comment: String,
    pub trust: Trust,
    /// The addresses the switcher may manage, e.g. "192.168.88.20-192.168.88.99".
    pub allowed_range: Option<AddressRange>,
}

fn required(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<String, String> {
    match get(key) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("{key} is required")),
    }
}

fn parsed<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    key: &str,
    what: &str,
) -> Result<Option<T>, String> {
    get(key)
        .map(|v| {
            v.trim()
                .parse()
                .map_err(|_| format!("{key}: invalid {what}: {v:?}"))
        })
        .transpose()
}

fn boolean(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<bool, String> {
    match get(key).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        None => Ok(false),
        Some("true" | "1" | "yes" | "on" | "y" | "enabled") => Ok(true),
        Some("false" | "0" | "no" | "off" | "n" | "disabled") => Ok(false),
        Some(other) => Err(format!("{key}: invalid boolean: {other:?}")),
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        Ok(Config {
            mikrotik_host: required(&get, "MIKROTIK_HOST")?,
            mikrotik_username: required(&get, "MIKROTIK_USERNAME")?,
            mikrotik_password: required(&get, "MIKROTIK_PASSWORD")?,
            mikrotik_tls_ca: get("MIKROTIK_TLS_CA").map(PathBuf::from),
            port: parsed(&get, "PORT", "port")?.unwrap_or(3000),
            custom_dns: required(&get, "CUSTOM_DNS")?
                .trim()
                .parse()
                .map_err(|_| "CUSTOM_DNS: invalid DNS server IP (IPv4 expected)".to_string())?,
            comment: get("APP_COMMENT")
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| "DNS-Switcher-Managed".into()),
            trust: Trust {
                proxy: parsed(&get, "TRUSTED_PROXY", "trusted proxy IP (IPv4 expected)")?,
                allow_target_ip: boolean(&get, "ALLOW_TARGET_IP")?,
            },
            allowed_range: get("ALLOWED_RANGE")
                .map(|v| AddressRange::parse(&v).map_err(|e| format!("ALLOWED_RANGE: {e}")))
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn load(extra: &[(&str, &str)]) -> Result<Config, String> {
        let mut env: HashMap<String, String> = [
            ("MIKROTIK_HOST", "192.168.88.1"),
            ("MIKROTIK_USERNAME", "u"),
            ("MIKROTIK_PASSWORD", "p"),
            ("CUSTOM_DNS", "192.168.88.113"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        Config::from_lookup(|k| env.get(k).cloned())
    }

    #[test]
    fn defaults() {
        let c = load(&[]).unwrap();
        assert_eq!(c.port, 3000);
        assert_eq!(c.comment, "DNS-Switcher-Managed");
        assert!(c.allowed_range.is_none() && c.trust.proxy.is_none() && !c.trust.allow_target_ip);
        assert!(c.mikrotik_tls_ca.is_none());
    }

    #[test]
    fn reads_the_production_settings() {
        let c = load(&[
            ("TRUSTED_PROXY", "172.19.0.100"),
            ("ALLOWED_RANGE", "192.168.88.20-192.168.88.99"),
            ("MIKROTIK_TLS_CA", "/ca.crt"),
            ("PORT", "8080"),
            ("ALLOW_TARGET_IP", "No"),
        ])
        .unwrap();
        assert_eq!(c.port, 8080);
        assert_eq!(c.trust.proxy, Some(Ipv4Addr::new(172, 19, 0, 100)));
        assert!(c.allowed_range.is_some());
        assert!(!c.trust.allow_target_ip);
    }

    #[test]
    fn missing_required_settings_are_errors() {
        for key in [
            "MIKROTIK_HOST",
            "MIKROTIK_USERNAME",
            "MIKROTIK_PASSWORD",
            "CUSTOM_DNS",
        ] {
            let err = Config::from_lookup(|k| {
                (k != key && k != "PORT").then(|| "192.168.88.1".to_string())
            })
            .unwrap_err();
            assert!(err.contains(key), "{err}");
        }
    }

    #[test]
    fn a_setting_that_is_set_but_wrong_never_falls_back_to_unset() {
        for (key, bad) in [
            ("ALLOWED_RANGE", "192.168.88.20"),
            ("ALLOWED_RANGE", ""),
            ("TRUSTED_PROXY", "nginx"),
            ("TRUSTED_PROXY", ""),
            ("ALLOW_TARGET_IP", "maybe"),
            ("PORT", "http"),
            ("CUSTOM_DNS", "::1"),
        ] {
            assert!(load(&[(key, bad)]).is_err(), "{key}={bad:?} should fail");
        }
    }
}
