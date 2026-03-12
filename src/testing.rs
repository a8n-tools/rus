//! Shared test utilities — compiled only when running `cargo test`.

use actix_web::web;
use crate::config::Config;
use crate::db::AppState;

/// JWT secret used in standalone tests (must be >32 chars for HS256).
#[cfg(feature = "standalone")]
pub const TEST_JWT_SECRET: &str = "test-secret-at-least-32-chars-ok!";

/// SaaS JWT secret used in SaaS tests.
#[cfg(feature = "saas")]
pub const TEST_SAAS_SECRET: &str = "test-saas-secret-32-chars-padded!";

/// Standard test password meeting all complexity requirements.
pub const TEST_PASSWORD: &str = "TestPass1!";

/// Create a Config suitable for testing (in-memory SQLite).
pub fn test_config() -> Config {
    Config {
        max_url_length: 2048,
        click_retention_days: 30,
        host_url: "http://localhost:8080".to_string(),
        db_path: ":memory:".to_string(),
        host: "127.0.0.1".to_string(),
        port: 8080,
        #[cfg(feature = "standalone")]
        jwt_secret: TEST_JWT_SECRET.to_string(),
        #[cfg(feature = "standalone")]
        jwt_expiry_hours: 1,
        #[cfg(feature = "standalone")]
        refresh_token_expiry_days: 7,
        #[cfg(feature = "standalone")]
        account_lockout_attempts: 5,
        #[cfg(feature = "standalone")]
        account_lockout_duration_minutes: 30,
        #[cfg(feature = "standalone")]
        allow_registration: true,
        #[cfg(feature = "saas")]
        saas_jwt_secret: TEST_SAAS_SECRET.to_string(),
        #[cfg(feature = "saas")]
        saas_login_url: "https://app.example.com/login".to_string(),
        #[cfg(feature = "saas")]
        saas_logout_url: "https://api.example.com/logout".to_string(),
        #[cfg(feature = "saas")]
        saas_membership_url: "https://app.example.com/membership".to_string(),
    }
}

/// Create an AppState backed by an in-memory SQLite database.
pub fn make_test_state() -> web::Data<AppState> {
    web::Data::new(AppState::new(test_config()).expect("Failed to create test AppState"))
}

/// Insert a user directly into the DB (bypasses password hashing — use for non-login tests).
/// Returns the new user_id.
#[cfg(feature = "standalone")]
pub fn insert_test_user(state: &web::Data<AppState>, username: &str, is_admin: bool) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO users (username, password, is_admin) VALUES (?1, 'placeholder', ?2)",
        rusqlite::params![username, is_admin as i32],
    )
    .expect("insert_test_user failed");
    db.last_insert_rowid()
}

/// Create a JWT token for use in standalone tests.
#[cfg(feature = "standalone")]
pub fn make_test_token(username: &str, user_id: i64, is_admin: bool) -> String {
    crate::auth::jwt::create_jwt(username, user_id, is_admin, TEST_JWT_SECRET, 1)
        .expect("make_test_token failed")
}

/// Insert a URL directly for standalone tests. Returns the new row id.
#[cfg(feature = "standalone")]
pub fn insert_test_url(
    state: &web::Data<AppState>,
    user_id: i64,
    original_url: &str,
    short_code: &str,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, original_url, short_code],
    )
    .expect("insert_test_url failed");
    db.last_insert_rowid()
}

/// Create a signed SaaS access_token JWT for use in cookie-based tests.
///
/// `user_id` should be a parseable integer string (e.g. `"42"`).
#[cfg(feature = "saas")]
pub fn make_saas_jwt(
    user_id: &str,
    email: &str,
    membership_status: &str,
    role: Option<&str>,
) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    let exp = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
    let mut claims = serde_json::json!({
        "sub": user_id,
        "email": email,
        "membership_status": membership_status,
        "exp": exp
    });
    if let Some(r) = role {
        claims["role"] = serde_json::Value::String(r.to_string());
    }

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_SAAS_SECRET.as_bytes()),
    )
    .expect("make_saas_jwt failed")
}

/// Insert a SaaS user directly into the local DB.
#[cfg(feature = "saas")]
pub fn insert_saas_user(state: &web::Data<AppState>, user_id: i64, username: &str, is_admin: bool) {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT OR IGNORE INTO users (userID, username, password, is_admin) VALUES (?1, ?2, '', ?3)",
        rusqlite::params![user_id, username, is_admin as i32],
    )
    .expect("insert_saas_user failed");
}

/// Compute HMAC-SHA256 signature for a webhook payload body.
#[cfg(feature = "saas")]
pub fn sign_webhook_payload(body: &[u8], secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key error");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Insert a URL directly for SaaS tests. Returns the new row id.
#[cfg(feature = "saas")]
pub fn insert_saas_url(
    state: &web::Data<AppState>,
    user_id: i64,
    original_url: &str,
    short_code: &str,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, original_url, short_code],
    )
    .expect("insert_saas_url failed");
    db.last_insert_rowid()
}
