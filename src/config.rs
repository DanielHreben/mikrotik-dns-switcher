use std::env;

#[derive(Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub mikrotik_host: String,
    pub mikrotik_port: u16,
    pub mikrotik_username: String,
    pub mikrotik_password: String,
    pub custom_dns: String,
    pub app_comment: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // Load .env file if it exists (only in debug builds)
        #[cfg(debug_assertions)]
        {
            match dotenv::dotenv() {
                Ok(path) => println!("Loaded .env file from: {}", path.display()),
                Err(_) => println!("No .env file found, using system environment variables"),
            }
        }

        Ok(Config {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),

            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .map_err(|_| "Invalid PORT value")?,

            mikrotik_host: env::var("MIKROTIK_HOST").unwrap_or_else(|_| "192.168.88.1".to_string()),

            mikrotik_port: env::var("MIKROTIK_PORT")
                .unwrap_or_else(|_| "8728".to_string())
                .parse()
                .map_err(|_| "Invalid MIKROTIK_PORT value")?,

            mikrotik_username: env::var("MIKROTIK_USERNAME")
                .map_err(|_| "MIKROTIK_USERNAME environment variable is required")?,

            mikrotik_password: env::var("MIKROTIK_PASSWORD")
                .map_err(|_| "MIKROTIK_PASSWORD environment variable is required")?,

            custom_dns: env::var("CUSTOM_DNS").unwrap_or_else(|_| "8.8.8.8".to_string()),

            app_comment: env::var("APP_COMMENT")
                .unwrap_or_else(|_| "DNS-Switcher-Managed".to_string()),
        })
    }
}
