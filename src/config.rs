use std::env;

/// Application configuration loaded from environment variables
#[derive(Clone, Debug)]
pub struct Config {
    #[cfg(feature = "standalone")]
    pub jwt_secret: String,
    #[cfg(feature = "standalone")]
    pub jwt_expiry_hours: i64,
    #[cfg(feature = "standalone")]
    pub refresh_token_expiry_days: i64,
    pub max_url_length: usize,
    #[cfg(feature = "standalone")]
    pub account_lockout_attempts: i32,
    #[cfg(feature = "standalone")]
    pub account_lockout_duration_minutes: i64,
    pub click_retention_days: i64,
    pub host_url: String,
    pub db_path: String,
    pub host: String,
    pub port: u16,
    #[cfg(feature = "standalone")]
    pub allow_registration: bool,
    #[cfg(feature = "saas")]
    pub saas_jwt_secret: String,
    #[cfg(feature = "saas")]
    pub saas_login_url: String,
    #[cfg(feature = "saas")]
    pub saas_logout_url: String,
    #[cfg(feature = "saas")]
    pub saas_membership_url: String,
    #[cfg(feature = "saas")]
    pub saas_refresh_url: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        #[cfg(feature = "standalone")]
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            eprintln!("WARNING: JWT_SECRET not set in environment, using default (insecure)");
            "your-secret-key-change-this-in-production".to_string()
        });

        #[cfg(feature = "standalone")]
        let jwt_expiry_hours = env::var("JWT_EXPIRY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        #[cfg(feature = "standalone")]
        let refresh_token_expiry_days = env::var("REFRESH_TOKEN_EXPIRY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7);

        let max_url_length = env::var("MAX_URL_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2048);

        #[cfg(feature = "standalone")]
        let account_lockout_attempts = env::var("ACCOUNT_LOCKOUT_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        #[cfg(feature = "standalone")]
        let account_lockout_duration_minutes = env::var("ACCOUNT_LOCKOUT_DURATION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let click_retention_days = env::var("CLICK_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let host_url = env::var("HOST_URL")
            .unwrap_or_else(|_| "http://localhost:4001".to_string());

        let db_path = env::var("DB_PATH")
            .unwrap_or_else(|_| "./data/rus.db".to_string());

        let host = env::var("HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = env::var("APP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4001);

        #[cfg(feature = "standalone")]
        let allow_registration = env::var("ALLOW_REGISTRATION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);

        #[cfg(feature = "saas")]
        let saas_jwt_secret = env::var("SAAS_JWT_SECRET")
            .expect("SAAS_JWT_SECRET must be set in SaaS mode");

        #[cfg(feature = "saas")]
        let saas_login_url = env::var("SAAS_LOGIN_URL")
            .unwrap_or_else(|_| "https://app.a8n.run".to_string());

        #[cfg(feature = "saas")]
        let saas_logout_url = env::var("SAAS_LOGOUT_URL")
            .unwrap_or_else(|_| "https://api.a8n.run/v1/auth/logout".to_string());

        #[cfg(feature = "saas")]
        let saas_membership_url = env::var("SAAS_MEMBERSHIP_URL")
            .unwrap_or_else(|_| "https://app.a8n.run/membership".to_string());

        #[cfg(feature = "saas")]
        let saas_refresh_url = env::var("SAAS_REFRESH_URL")
            .unwrap_or_else(|_| saas_logout_url.replace("/auth/logout", "/auth/refresh"));

        Config {
            #[cfg(feature = "standalone")]
            jwt_secret,
            #[cfg(feature = "standalone")]
            jwt_expiry_hours,
            #[cfg(feature = "standalone")]
            refresh_token_expiry_days,
            max_url_length,
            #[cfg(feature = "standalone")]
            account_lockout_attempts,
            #[cfg(feature = "standalone")]
            account_lockout_duration_minutes,
            click_retention_days,
            host_url,
            db_path,
            host,
            port,
            #[cfg(feature = "standalone")]
            allow_registration,
            #[cfg(feature = "saas")]
            saas_jwt_secret,
            #[cfg(feature = "saas")]
            saas_login_url,
            #[cfg(feature = "saas")]
            saas_logout_url,
            #[cfg(feature = "saas")]
            saas_membership_url,
            #[cfg(feature = "saas")]
            saas_refresh_url,
        }
    }

    /// Get JWT secret (helper method for compatibility) - standalone only
    #[cfg(feature = "standalone")]
    pub fn get_jwt_secret() -> String {
        env::var("JWT_SECRET").unwrap_or_else(|_| {
            eprintln!("WARNING: JWT_SECRET not set in environment, using default (insecure)");
            "your-secret-key-change-this-in-production".to_string()
        })
    }

    /// Print configuration banner on startup
    pub fn print_banner(&self) {
        #[cfg(feature = "standalone")]
        {
            println!("========================================");
            println!("  Rust URL Shortener - Standalone Mode");
            println!("========================================");
            println!("Host: {}", self.host);
            println!("Port: {}", self.port);
            println!("Host URL: {}", self.host_url);
            println!("Database Path: {}", self.db_path);
            println!("JWT Expiry: {} hours", self.jwt_expiry_hours);
            println!("Refresh Token Expiry: {} days", self.refresh_token_expiry_days);
            println!("Max URL Length: {}", self.max_url_length);
            println!("Account Lockout Attempts: {}", self.account_lockout_attempts);
            println!("Account Lockout Duration: {} minutes", self.account_lockout_duration_minutes);
            println!("Click Retention: {} days", self.click_retention_days);
            println!("Allow Registration: {}", self.allow_registration);
            println!("========================================");
        }

        #[cfg(feature = "saas")]
        {
            println!("========================================");
            println!("  Rust URL Shortener - SaaS Mode");
            println!("========================================");
            println!("Host: {}", self.host);
            println!("Port: {}", self.port);
            println!("Host URL: {}", self.host_url);
            println!("Database Path: {}", self.db_path);
            println!("Max URL Length: {}", self.max_url_length);
            println!("Click Retention: {} days", self.click_retention_days);
            println!("SAAS JWT Secret: [set]");
            println!("========================================");
        }
    }
}
