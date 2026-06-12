//! Shared test utilities - compiled only when running `cargo test`.

use crate::config::Config;
use crate::db::AppState;
use actix_web::web;

/// JWT secret used in standalone tests (must be >32 chars for HS256).
#[cfg(feature = "standalone")]
pub const TEST_JWT_SECRET: &str = "test-secret-at-least-32-chars-ok!";

/// Webhook HMAC secret used in SaaS tests.
#[cfg(feature = "saas")]
pub const TEST_WEBHOOK_SECRET: &str = "test-webhook-secret-32-chars-pad!";

/// Standard test password meeting all complexity requirements.
#[cfg(feature = "standalone")]
pub const TEST_PASSWORD: &str = "TestPass1!";

/// Create a Config suitable for testing (in-memory SQLite).
pub fn test_config() -> Config {
    Config {
        max_url_length: 2048,
        click_retention_days: 30,
        host_url: "http://localhost:4001".to_string(),
        db_path: ":memory:".to_string(),
        host: "127.0.0.1".to_string(),
        port: 4001,
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
        webhook_secret: TEST_WEBHOOK_SECRET.to_string(),
        #[cfg(feature = "saas")]
        oidc: crate::config::OidcConfig {
            issuer: String::new(),
            audience: "http://localhost:4001/api".to_string(),
            jwks_url: String::new(),
            jwks_cache_ttl: 300,
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            redirect_uri: "http://localhost:4001/oauth2/callback".to_string(),
            post_logout_redirect_uri: "http://localhost:4001/".to_string(),
            leeway_seconds: 30,
            lifecycle_jti_cache_ttl: 300,
            session_ttl_seconds: 1_209_600,
        },
    }
}

/// Create an AppState backed by an in-memory SQLite database.
pub fn make_test_state() -> web::Data<AppState> {
    web::Data::new(AppState::new(test_config()).expect("Failed to create test AppState"))
}

/// Insert a user directly into the DB (bypasses password hashing).
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

/// Insert a SaaS user directly into the local DB. Returns the new user_id.
#[cfg(feature = "saas")]
pub fn insert_saas_user(
    state: &web::Data<AppState>,
    username: &str,
    saas_user_id: &str,
    is_admin: bool,
) -> i64 {
    let db = state.db.lock().unwrap();
    db.execute(
        "INSERT INTO users (username, password, is_admin, saas_user_id, email)
         VALUES (?1, '!sso:no-password', ?2, ?3, ?4)",
        rusqlite::params![
            username,
            is_admin as i32,
            saas_user_id,
            format!("{username}@example.com")
        ],
    )
    .expect("insert_saas_user failed");
    db.last_insert_rowid()
}

/// Create a BFF session for the given user and return the raw cookie value.
#[cfg(feature = "saas")]
pub fn make_saas_session(state: &web::Data<AppState>, user_id: i64) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use chrono::Utc;
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    let token = URL_SAFE_NO_PAD.encode(buf);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();

    let db = state.db.lock().unwrap();
    let session_version: i32 = db
        .query_row(
            "SELECT session_version FROM users WHERE userID = ?1",
            rusqlite::params![user_id],
            |r| r.get(0),
        )
        .expect("user not found");

    let now = Utc::now();
    let expires = now + chrono::Duration::hours(1);
    db.execute(
        "INSERT INTO user_sessions (id, session_token_hash, user_id, session_version, auth_via_oidc, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            token_hash,
            user_id,
            session_version,
            now.to_rfc3339(),
            expires.to_rfc3339()
        ],
    )
    .expect("insert user_sessions failed");

    token
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

/// Build an `IdTokenClaims` fixture for JIT tests.
#[cfg(feature = "saas")]
pub fn id_claims(
    sub: &str,
    email: Option<&str>,
    email_verified: bool,
    has_member_access: bool,
    role: Option<&str>,
) -> crate::oidc::verifier::IdTokenClaims {
    crate::oidc::verifier::IdTokenClaims {
        iss: "https://idp.example.com".to_string(),
        sub: sub.to_string(),
        aud: serde_json::json!("rus-test-client"),
        exp: 0,
        iat: 0,
        nonce: Some("test-nonce".to_string()),
        email: email.map(String::from),
        email_verified: Some(email_verified),
        name: None,
        role: role.map(String::from),
        has_member_access: Some(has_member_access),
    }
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
