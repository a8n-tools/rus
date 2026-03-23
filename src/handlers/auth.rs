use actix_web::{web, HttpRequest, HttpResponse, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use rusqlite::params;
use tracing::{debug, error, info, warn};

use crate::auth::get_claims;
use crate::auth::jwt::{create_jwt, generate_refresh_token};
use crate::db::AppState;
use crate::models::{
    AuthResponse, CurrentUserResponse, LoginRequest, RefreshRequest, RefreshResponse,
    RegisterRequest,
};
use crate::security::{is_account_locked, record_login_attempt, validate_password};

/// User registration endpoint
pub async fn register(
    data: web::Data<AppState>,
    req: web::Json<RegisterRequest>,
) -> Result<HttpResponse> {
    // Validate input before acquiring the lock
    if req.username.is_empty() || req.password.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Username and password cannot be empty"
        })));
    }

    if req.username.len() < 3 {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Username must be at least 3 characters"
        })));
    }

    // Validate username characters (alphanumeric, underscores, hyphens only)
    if !req.username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Username can only contain letters, numbers, underscores, and hyphens"
        })));
    }

    // Validate password complexity
    if let Err(error_message) = validate_password(&req.password) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })));
    }

    // Hash password before acquiring the lock (expensive operation)
    let hashed_password = match hash_password(&req.password) {
        Ok(h) => h,
        Err(_) => {
            error!(username = %req.username, "Password hashing failed");
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to hash password"
            })));
        }
    };

    // Acquire lock once for all DB operations (prevents TOCTOU race)
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Check registration allowed + first user in a single lock scope
    let user_count: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    if !data.config.allow_registration && user_count > 0 {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": "New user registration is disabled. Please contact the administrator."
        })));
    }

    let is_admin = user_count == 0;

    match db.execute(
        "INSERT INTO users (username, password, is_admin) VALUES (?1, ?2, ?3)",
        params![&req.username, &hashed_password, is_admin as i32],
    ) {
        Ok(_) => {
            // Get the user ID
            let user_id: i64 = db.last_insert_rowid();

            // Create JWT token
            match create_jwt(
                &req.username,
                user_id,
                is_admin,
                &data.config.jwt_secret,
                data.config.jwt_expiry_hours,
            ) {
                Ok(token) => {
                    // Create refresh token
                    let refresh_token = generate_refresh_token();
                    let expires_at =
                        Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
                    let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

                    let _ = db.execute(
                        "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                        params![user_id, &refresh_token, &expires_at_str],
                    );

                    info!(username = %req.username, user_id, is_admin, "User registered");
                    Ok(HttpResponse::Created().json(AuthResponse {
                        token,
                        refresh_token,
                        username: req.username.clone(),
                    }))
                }
                Err(_) => {
                    error!(username = %req.username, "Failed to create JWT after registration");
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create token"
                    })))
                }
            }
        }
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                warn!(username = %req.username, "Registration failed: username already exists");
                Ok(HttpResponse::Conflict().json(serde_json::json!({
                    "error": "Username already exists"
                })))
            } else {
                error!(username = %req.username, error = %e, "Failed to create user");
                Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Failed to create user"
                })))
            }
        }
    }
}

/// User login endpoint
pub async fn login(
    data: web::Data<AppState>,
    req: web::Json<LoginRequest>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Check for account lockout BEFORE any other database operations
    // This prevents timing attacks that could reveal if a username exists
    if is_account_locked(
        &db,
        &req.username,
        data.config.account_lockout_attempts,
        data.config.account_lockout_duration_minutes,
    ) {
        warn!(username = %req.username, "Login blocked: account locked");
        return Ok(HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": format!(
                "Account locked due to too many failed attempts. Try again in {} minutes.",
                data.config.account_lockout_duration_minutes
            )
        })));
    }

    // Get user from database
    let mut stmt = match db.prepare(
        "SELECT userID, username, password, is_admin FROM users WHERE username = ?1",
    ) {
        Ok(stmt) => stmt,
        Err(_) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Database error"
            })));
        }
    };

    let user_result: rusqlite::Result<(i64, String, String, i32)> = stmt.query_row(
        params![&req.username],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    );

    match user_result {
        Ok((user_id, username, hashed_password, is_admin_int)) => {
            let is_admin = is_admin_int != 0;
            // Verify password (supports both Argon2id and legacy bcrypt hashes)
            match verify_password(&req.password, &hashed_password) {
                Ok(true) => {
                    // Opportunistically rehash legacy bcrypt passwords to Argon2id
                    if is_legacy_bcrypt_hash(&hashed_password) {
                        if let Ok(new_hash) = hash_password(&req.password) {
                            let _ = db.execute(
                                "UPDATE users SET password = ?1 WHERE userID = ?2",
                                params![&new_hash, user_id],
                            );
                        }
                    }
                    // Record successful login attempt
                    record_login_attempt(&db, &req.username, true);
                    // Create JWT token
                    match create_jwt(
                        &username,
                        user_id,
                        is_admin,
                        &data.config.jwt_secret,
                        data.config.jwt_expiry_hours,
                    ) {
                        Ok(token) => {
                            // Create refresh token
                            let refresh_token = generate_refresh_token();
                            let expires_at = Utc::now()
                                + Duration::days(data.config.refresh_token_expiry_days);
                            let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

                            let _ = db.execute(
                                "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                                params![user_id, &refresh_token, &expires_at_str],
                            );

                            info!(username = %username, user_id, "User logged in");
                            Ok(HttpResponse::Ok().json(AuthResponse {
                                token,
                                refresh_token,
                                username,
                            }))
                        }
                        Err(_) => {
                            error!(username = %username, "Failed to create JWT after login");
                            Ok(HttpResponse::InternalServerError().json(
                                serde_json::json!({
                                    "error": "Failed to create token"
                                }),
                            ))
                        }
                    }
                }
                Ok(false) => {
                    // Record failed login attempt (wrong password)
                    record_login_attempt(&db, &req.username, false);
                    warn!(username = %req.username, "Login failed: invalid password");
                    Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "Invalid credentials"
                    })))
                }
                Err(_) => {
                    error!(username = %req.username, "Password verification error");
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Authentication error"
                    })))
                }
            }
        }
        Err(_) => {
            // Record failed login attempt (user not found)
            record_login_attempt(&db, &req.username, false);
            warn!(username = %req.username, "Login failed: user not found");
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid credentials"
            })))
        }
    }
}

/// Token refresh endpoint
pub async fn refresh_token(
    data: web::Data<AppState>,
    req: web::Json<RefreshRequest>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    // Find and validate refresh token
    let token_result: rusqlite::Result<(i64, i64, String, i32)> = db.query_row(
        "SELECT rt.id, rt.user_id, u.username, u.is_admin FROM refresh_tokens rt
         JOIN users u ON rt.user_id = u.userID
         WHERE rt.token = ?1 AND rt.expires_at > datetime('now')",
        params![&req.refresh_token],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    );

    match token_result {
        Ok((token_id, user_id, username, is_admin_int)) => {
            let is_admin = is_admin_int != 0;
            // Delete old refresh token (rotation)
            let _ = db.execute(
                "DELETE FROM refresh_tokens WHERE id = ?1",
                params![token_id],
            );

            // Create new JWT token
            let token = match create_jwt(
                &username,
                user_id,
                is_admin,
                &data.config.jwt_secret,
                data.config.jwt_expiry_hours,
            ) {
                Ok(t) => t,
                Err(_) => {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create token"
                    })));
                }
            };

            // Create new refresh token (rotation)
            let new_refresh_token = generate_refresh_token();
            let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
            let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

            let _ = db.execute(
                "INSERT INTO refresh_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![user_id, &new_refresh_token, &expires_at_str],
            );

            debug!(user_id, "Token refreshed");
            Ok(HttpResponse::Ok().json(RefreshResponse {
                token,
                refresh_token: new_refresh_token,
            }))
        }
        Err(_) => {
            warn!("Token refresh failed: invalid or expired refresh token");
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid or expired refresh token"
            })))
        }
    }
}

/// Hash a password using Argon2id
fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2.hash_password(password.as_bytes(), &salt)?.to_string())
}

/// Check if a stored hash is a legacy bcrypt hash
fn is_legacy_bcrypt_hash(hash: &str) -> bool {
    hash.starts_with("$2b$") || hash.starts_with("$2a$") || hash.starts_with("$2y$")
}

/// Verify a password against a hash, supporting both Argon2id and legacy bcrypt
fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    if is_legacy_bcrypt_hash(hash) {
        bcrypt::verify(password, hash).map_err(|e| e.to_string())
    } else {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|e| e.to_string())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

/// Get current user info
pub async fn get_current_user(http_req: HttpRequest) -> Result<HttpResponse> {
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    Ok(HttpResponse::Ok().json(CurrentUserResponse {
        user_id: claims.user_id,
        username: claims.sub,
        is_admin: claims.is_admin,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use actix_web_httpauth::middleware::HttpAuthentication;
    use crate::auth::middleware::jwt_validator;
    use crate::testing::{make_test_state, TEST_PASSWORD};
    use serde_json::Value;

    macro_rules! setup_app {
        ($state:expr) => {{
            let jwt = HttpAuthentication::bearer(jwt_validator);
            test::init_service(
                App::new()
                    .app_data($state.clone())
                    .route("/api/register", web::post().to(register))
                    .route("/api/login", web::post().to(login))
                    .route("/api/refresh", web::post().to(refresh_token))
                    .service(
                        web::scope("/api")
                            .wrap(jwt)
                            .route("/me", web::get().to(get_current_user)),
                    ),
            )
            .await
        }};
    }

    /// Register a user and return the token.
    async fn do_register(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        username: &str,
    ) -> Value {
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": username, "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(app, req).await;
        test::read_body_json(resp).await
    }

    // --- register ---

    #[actix_web::test]
    async fn register_success_returns_201() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn register_returns_token_and_username() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        assert!(body["token"].is_string());
        assert!(body["refresh_token"].is_string());
        assert_eq!(body["username"], "alice");
    }

    #[actix_web::test]
    async fn register_first_user_is_admin() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["is_admin"], true);
    }

    #[actix_web::test]
    async fn register_second_user_not_admin() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        let body = do_register(&app, "bob").await;
        let token = body["token"].as_str().unwrap();

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["is_admin"], false);
    }

    #[actix_web::test]
    async fn register_duplicate_username_returns_409() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;

        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 409);
    }

    #[actix_web::test]
    async fn register_empty_username_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn register_short_username_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "ab", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn register_invalid_username_chars_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "user@name", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn register_weak_password_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "alice", "password": "weak"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn register_disabled_blocks_second_user() {
        let mut config = crate::testing::test_config();
        config.allow_registration = false;
        let state = web::Data::new(crate::db::AppState::new(config).unwrap());
        let app = setup_app!(state);

        // First user always allowed
        let resp_first = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/register")
                .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
                .to_request(),
        )
        .await;
        assert_eq!(resp_first.status(), 201);

        // Second user blocked
        let resp_second = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/register")
                .set_json(serde_json::json!({"username": "bob", "password": TEST_PASSWORD}))
                .to_request(),
        )
        .await;
        assert_eq!(resp_second.status(), 403);
    }

    // --- login ---

    #[actix_web::test]
    async fn login_success_returns_token() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;

        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
    }

    #[actix_web::test]
    async fn login_wrong_password_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;

        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "alice", "password": "WrongPass1!"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn login_unknown_user_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "nobody", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn login_account_lockout_after_five_failures() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;

        for _ in 0..5 {
            test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/api/login")
                    .set_json(serde_json::json!({"username": "alice", "password": "WrongPass1!"}))
                    .to_request(),
            )
            .await;
        }

        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 429);
    }

    // --- refresh_token ---

    #[actix_web::test]
    async fn refresh_token_rotation_works() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let old_refresh = body["refresh_token"].as_str().unwrap().to_string();

        let req = test::TestRequest::post()
            .uri("/api/refresh")
            .set_json(serde_json::json!({"refresh_token": old_refresh}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
        // New refresh token must differ (rotation)
        assert_ne!(body["refresh_token"].as_str().unwrap(), old_refresh);
    }

    #[actix_web::test]
    async fn refresh_token_invalid_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/refresh")
            .set_json(serde_json::json!({"refresh_token": "invalid-refresh-token"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn refresh_token_cant_be_reused() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let old_refresh = body["refresh_token"].as_str().unwrap().to_string();

        // Use it once
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/refresh")
                .set_json(serde_json::json!({"refresh_token": old_refresh}))
                .to_request(),
        )
        .await;

        // Second use must fail
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/refresh")
                .set_json(serde_json::json!({"refresh_token": old_refresh}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 401);
    }

    // --- get_current_user ---

    #[actix_web::test]
    async fn get_me_returns_username() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["username"], "alice");
    }

    #[actix_web::test]
    async fn get_me_without_token_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::get().uri("/api/me").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn get_me_with_bad_token_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", "Bearer not.a.valid.token"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn register_empty_password_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({"username": "alice", "password": ""}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn login_with_bcrypt_hash_succeeds_and_rehashes() {
        let state = make_test_state();
        let app = setup_app!(state);

        // Insert user with a bcrypt-hashed password directly in DB
        let bcrypt_hash = bcrypt::hash(TEST_PASSWORD, bcrypt::DEFAULT_COST).unwrap();
        {
            let db = state.db.lock().unwrap();
            db.execute(
                "INSERT INTO users (username, password, is_admin) VALUES ('alice', ?1, 0)",
                rusqlite::params![&bcrypt_hash],
            )
            .unwrap();
        }

        // Login should succeed
        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // Verify the hash was migrated to argon2id
        let new_hash: String = {
            let db = state.db.lock().unwrap();
            db.query_row(
                "SELECT password FROM users WHERE username = 'alice'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(
            new_hash.starts_with("$argon2id$"),
            "expected argon2id hash, got: {}",
            &new_hash[..20]
        );
    }

    #[actix_web::test]
    async fn login_returns_refresh_token() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;

        let req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": "alice", "password": TEST_PASSWORD}))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        assert!(body["refresh_token"].is_string());
        assert!(!body["refresh_token"].as_str().unwrap().is_empty());
    }
}
