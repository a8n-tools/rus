use std::env;

/// OIDC Relying Party + Resource Server configuration (saas mode).
#[cfg(feature = "saas")]
#[derive(Clone, Debug)]
pub struct OidcConfig {
    /// Issuer URL (`iss` value in tokens). Empty string means OIDC disabled.
    pub issuer: String,
    /// `aud` expected in `at+jwt` access tokens.
    pub audience: String,
    /// JWKS endpoint (derived from issuer when empty).
    pub jwks_url: String,
    /// JWKS in-memory cache TTL in seconds.
    pub jwks_cache_ttl: u64,
    /// OAuth2 client_id.
    pub client_id: String,
    /// OAuth2 client_secret (confidential client).
    pub client_secret: String,
    /// Absolute redirect URI registered with the OP.
    pub redirect_uri: String,
    /// Post-logout redirect URI registered with the OP.
    pub post_logout_redirect_uri: String,
    /// Clock-skew leeway in seconds applied during token validation.
    pub leeway_seconds: u64,
    /// TTL in seconds for the JTI idempotency cache (lifecycle + logout events).
    pub lifecycle_jti_cache_ttl: u64,
    /// Lifetime in seconds for BFF `rus_session` cookies.
    pub session_ttl_seconds: u64,
}

#[cfg(feature = "saas")]
impl OidcConfig {
    pub fn enabled(&self) -> bool {
        !self.issuer.is_empty()
    }
}

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
    /// HMAC secret for the maintenance webhook (saas mode).
    #[cfg(feature = "saas")]
    pub webhook_secret: String,
    #[cfg(feature = "saas")]
    pub oidc: OidcConfig,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        #[cfg(feature = "standalone")]
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set in environment, using default (insecure)");
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

        let host = env::var("APP_HOST")
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
        let webhook_secret = env::var("WEBHOOK_SECRET").unwrap_or_else(|_| {
            tracing::warn!(
                "WEBHOOK_SECRET not set - maintenance webhook signatures will not validate"
            );
            String::new()
        });

        #[cfg(feature = "saas")]
        let oidc = build_oidc_config(&host_url);

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
            webhook_secret,
            #[cfg(feature = "saas")]
            oidc,
        }
    }

    /// Get JWT secret (helper method for compatibility) - standalone only
    #[cfg(feature = "standalone")]
    pub fn get_jwt_secret() -> String {
        env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set in environment, using default (insecure)");
            "your-secret-key-change-this-in-production".to_string()
        })
    }

    /// Print configuration banner on startup
    pub fn print_banner(&self) {
        #[cfg(feature = "standalone")]
        tracing::info!(
            mode = "standalone",
            host = %self.host,
            port = self.port,
            host_url = %self.host_url,
            db_path = %self.db_path,
            jwt_expiry_hours = self.jwt_expiry_hours,
            refresh_token_expiry_days = self.refresh_token_expiry_days,
            max_url_length = self.max_url_length,
            account_lockout_attempts = self.account_lockout_attempts,
            account_lockout_duration_minutes = self.account_lockout_duration_minutes,
            click_retention_days = self.click_retention_days,
            allow_registration = self.allow_registration,
            "RUS configuration loaded"
        );

        #[cfg(feature = "saas")]
        tracing::info!(
            mode = "saas",
            host = %self.host,
            port = self.port,
            host_url = %self.host_url,
            db_path = %self.db_path,
            max_url_length = self.max_url_length,
            click_retention_days = self.click_retention_days,
            oidc_enabled = self.oidc.enabled(),
            oidc_issuer = %self.oidc.issuer,
            "RUS configuration loaded"
        );
    }
}

#[cfg(feature = "saas")]
fn build_oidc_config(host_url: &str) -> OidcConfig {
    let host_url_trim = host_url.trim_end_matches('/').to_string();
    let issuer = env::var("OIDC_ISSUER").unwrap_or_default();

    let audience =
        env::var("OIDC_AUDIENCE").unwrap_or_else(|_| format!("{host_url_trim}/api"));

    let jwks_url = env::var("OIDC_JWKS_URL").unwrap_or_else(|_| {
        if issuer.is_empty() {
            String::new()
        } else {
            format!("{}/.well-known/jwks.json", issuer.trim_end_matches('/'))
        }
    });

    let jwks_cache_ttl = env::var("OIDC_JWKS_CACHE_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);

    let client_id = env::var("OIDC_CLIENT_ID").unwrap_or_default();

    let client_secret = env::var("OIDC_CLIENT_SECRET")
        .or_else(|_| {
            std::fs::read_to_string("/run/secrets/oidc_client_secret")
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_default();

    let redirect_uri = env::var("OIDC_REDIRECT_URI")
        .unwrap_or_else(|_| format!("{host_url_trim}/oauth2/callback"));

    let post_logout_redirect_uri = env::var("OIDC_POST_LOGOUT_REDIRECT_URI")
        .unwrap_or_else(|_| format!("{host_url_trim}/"));

    let leeway_seconds = env::var("OIDC_LEEWAY_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let lifecycle_jti_cache_ttl = env::var("OIDC_LIFECYCLE_JTI_CACHE_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);

    let session_ttl_seconds = env::var("OIDC_SESSION_TTL_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_209_600); // 14 days

    if !issuer.is_empty() && (client_id.is_empty() || client_secret.is_empty()) {
        tracing::error!(
            "OIDC_ISSUER is set but OIDC_CLIENT_ID or OIDC_CLIENT_SECRET is missing - OIDC will not function"
        );
    }

    if !jwks_url.is_empty()
        && !jwks_url.starts_with("https://")
        && !jwks_url.starts_with("http://localhost")
        && !jwks_url.starts_with("http://127.0.0.1")
    {
        tracing::warn!(
            jwks_url = %jwks_url,
            "OIDC_JWKS_URL is not HTTPS - acceptable only for local development"
        );
    }

    OidcConfig {
        issuer,
        audience,
        jwks_url,
        jwks_cache_ttl,
        client_id,
        client_secret,
        redirect_uri,
        post_logout_redirect_uri,
        leeway_seconds,
        lifecycle_jti_cache_ttl,
        session_ttl_seconds,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// test_config() is used throughout the test suite; verify it produces sane values.
    #[test]
    fn test_config_has_expected_defaults() {
        let cfg = crate::testing::test_config();
        assert_eq!(cfg.max_url_length, 2048);
        assert_eq!(cfg.click_retention_days, 30);
        assert_eq!(cfg.host_url, "http://localhost:4001");
        assert_eq!(cfg.db_path, ":memory:");
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 4001);
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn test_config_standalone_fields() {
        let cfg = crate::testing::test_config();
        assert_eq!(cfg.jwt_expiry_hours, 1);
        assert_eq!(cfg.refresh_token_expiry_days, 7);
        assert_eq!(cfg.account_lockout_attempts, 5);
        assert_eq!(cfg.account_lockout_duration_minutes, 30);
        assert!(cfg.allow_registration);
        assert!(!cfg.jwt_secret.is_empty());
    }

    #[cfg(feature = "saas")]
    #[test]
    fn test_config_saas_fields() {
        let cfg = crate::testing::test_config();
        assert!(!cfg.oidc.audience.is_empty());
        assert!(!cfg.oidc.redirect_uri.is_empty());
        assert!(cfg.oidc.session_ttl_seconds > 0);
    }

    #[test]
    fn config_is_clone_and_debug() {
        let cfg = crate::testing::test_config();
        let _cloned = cfg.clone();
        let debug_str = format!("{:?}", cfg);
        assert!(!debug_str.is_empty());
    }

    #[test]
    fn print_banner_does_not_panic() {
        let cfg = crate::testing::test_config();
        cfg.print_banner();
    }

    #[cfg(feature = "standalone")]
    #[test]
    fn get_jwt_secret_returns_default_when_unset() {
        let secret = Config::get_jwt_secret();
        assert!(!secret.is_empty());
    }
}
