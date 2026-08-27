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
use crate::login_approval::GateDecision;
use crate::mailer::normalize_account_email;
use crate::models::{
    CurrentUserResponse, LoginRequest, RefreshRequest, RefreshResponse, RegisterRequest,
    UpdateAccountRequest,
};
use crate::security::{is_account_locked, record_login_attempt, validate_password};

/// User registration endpoint
pub async fn register(
    data: web::Data<AppState>,
    req: web::Json<RegisterRequest>,
    http_req: HttpRequest,
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
    if !req
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
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

    // RUS-11: optional address for security notices; blank stores NULL.
    let email = match normalize_account_email(req.email.as_deref().unwrap_or_default()) {
        Ok(value) => value,
        Err(error_message) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": error_message
            })));
        }
    };

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
        "INSERT INTO users (username, password, is_admin, email) VALUES (?1, ?2, ?3, ?4)",
        params![
            &req.username,
            &hashed_password,
            is_admin as i32,
            email.as_deref()
        ],
    ) {
        Ok(_) => {
            // Get the user ID
            let user_id: i64 = db.last_insert_rowid();

            // RUS-19: no gate here. A brand-new account has no prior country,
            // so the predicate can never hold this sign-in; the alert below
            // still records the baseline the next one is compared against.
            match crate::auth::establish_session(
                &db,
                &data.config,
                &req.username,
                user_id,
                is_admin,
            ) {
                Ok(session) => {
                    info!(username = %req.username, user_id, is_admin, "User registered");
                    // RUS-7: records the baseline country so a later sign-in
                    // from elsewhere alerts. A first login can never alert.
                    crate::location_alert::spawn_new_location_check(&data, user_id, &http_req);
                    Ok(HttpResponse::Created().json(session))
                }
                Err(error) => {
                    error!(username = %req.username, error = %error, "Failed to establish a session after registration");
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
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Every database touch happens inside `login_locked`, which returns with the
    // connection lock released. RUS-19's approval mail is awaited out here, so an
    // SMTP round trip can never hold the single shared connection.
    match login_locked(&data, &req, &http_req) {
        LoginOutcome::Response(response) => Ok(response),
        LoginOutcome::Hold {
            user_id,
            username,
            context,
            recipient,
        } => Ok(
            match crate::login_approval::request_login_approval(
                &data, user_id, &context, &recipient, &http_req,
            )
            .await
            {
                Ok(()) => crate::login_approval::held_response(),
                Err(error) => {
                    error!(username = %username, error = %error, "RUS-19: approval mail failed, sign-in refused");
                    crate::login_approval::hold_failed_response()
                }
            },
        ),
    }
}

/// What the locked half of a login decided: an answer, or a sign-in to hold.
enum LoginOutcome {
    Response(HttpResponse),
    Hold {
        user_id: i64,
        username: String,
        context: crate::login_approval::GateContext,
        recipient: crate::mailer::AlertRecipient,
    },
}

/// The whole of a login that needs the database, start to finish under one lock.
fn login_locked(
    data: &web::Data<AppState>,
    req: &LoginRequest,
    http_req: &HttpRequest,
) -> LoginOutcome {
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
        return LoginOutcome::Response(HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": format!(
                "Account locked due to too many failed attempts. Try again in {} minutes.",
                data.config.account_lockout_duration_minutes
            )
        })));
    }

    // Get user from database. Scoped so the prepared statement stops borrowing
    // the connection.
    let user_result: rusqlite::Result<(i64, String, String, i32)> = {
        let mut stmt = match db
            .prepare("SELECT userID, username, password, is_admin FROM users WHERE username = ?1")
        {
            Ok(stmt) => stmt,
            Err(_) => {
                return LoginOutcome::Response(HttpResponse::InternalServerError().json(
                    serde_json::json!({
                        "error": "Database error"
                    }),
                ));
            }
        };

        stmt.query_row(params![&req.username], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
    };

    let Ok((user_id, username, hashed_password, is_admin_int)) = user_result else {
        // Record failed login attempt (user not found)
        record_login_attempt(&db, &req.username, false);
        warn!(username = %req.username, "Login failed: user not found");
        return LoginOutcome::Response(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid credentials"
        })));
    };
    let is_admin = is_admin_int != 0;

    // Verify password (supports both Argon2id and legacy bcrypt hashes)
    match verify_password(&req.password, &hashed_password) {
        Ok(true) => {}
        Ok(false) => {
            // Record failed login attempt (wrong password)
            record_login_attempt(&db, &req.username, false);
            warn!(username = %req.username, "Login failed: invalid password");
            return LoginOutcome::Response(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid credentials"
            })));
        }
        Err(_) => {
            error!(username = %req.username, "Password verification error");
            return LoginOutcome::Response(HttpResponse::InternalServerError().json(
                serde_json::json!({
                    "error": "Authentication error"
                }),
            ));
        }
    }

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

    // RUS-19: the password is right, so this is the point a session would be
    // established. Decide the gate before anything is minted.
    let gate = crate::login_approval::gate_login(&db, &data.config.mail, user_id, http_req);
    if let Some(context) = gate {
        match &context.decision {
            GateDecision::Hold(recipient) => {
                let recipient = recipient.clone();
                return LoginOutcome::Hold {
                    user_id,
                    username,
                    context,
                    recipient,
                };
            }
            // Gate-worthy with nothing to deliver the link with. Holding here
            // would be a lockout with no way back in, so the sign-in completes
            // and the RUS-7 alert carries the signal instead.
            GateDecision::AllowUndeliverable => {
                warn!(username = %username, user_id, "RUS-19: new-country sign-in allowed because no approval link could be delivered; set SMTP_HOST, SMTP_FROM_EMAIL and an account address or SECURITY_ALERT_EMAIL");
            }
            GateDecision::Allow => {}
        }
    }

    // RUS-7: alert on a sign-in from a new country, off the hot path.
    crate::location_alert::spawn_new_location_check(data, user_id, http_req);
    match crate::auth::establish_session(&db, &data.config, &username, user_id, is_admin) {
        Ok(session) => {
            info!(username = %username, user_id, "User logged in");
            LoginOutcome::Response(HttpResponse::Ok().json(session))
        }
        Err(error) => {
            error!(username = %username, error = %error, "Failed to establish a session after login");
            LoginOutcome::Response(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create token"
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
pub(crate) fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
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
        let parsed_hash = PasswordHash::new(hash).map_err(|e| e.to_string())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

/// Get current user info
pub async fn get_current_user(
    data: web::Data<AppState>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    // The account needs to see the address its security notices go to (RUS-11)
    // and whether its new-location alerts are on (RUS-15). One lock, both reads.
    let (email, notify_new_location) = {
        let db = data.db.lock().unwrap_or_else(|e| e.into_inner());
        let email: Option<String> = db
            .query_row(
                "SELECT email FROM users WHERE userID = ?1",
                params![claims.user_id],
                |row| row.get(0),
            )
            .unwrap_or(None);
        let notify = crate::location_alert::get_notify_new_location(&db, claims.user_id)
            .ok()
            .flatten()
            .unwrap_or(true);
        (email, notify)
    };

    Ok(HttpResponse::Ok().json(CurrentUserResponse {
        user_id: claims.user_id,
        username: claims.sub,
        is_admin: claims.is_admin,
        email,
        notify_new_location,
    }))
}

/// Account settings endpoint: set or clear the address security notices go to
/// (RUS-11, blank clears it back to NULL), and turn this account's
/// new-location sign-in alerts on or off (RUS-15). The account is always the
/// session's, never an id taken from the body.
pub async fn update_current_user(
    data: web::Data<AppState>,
    req: web::Json<UpdateAccountRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    // Both fields are optional and independent: an absent key means "not
    // submitted" and leaves the stored value alone, so a request that only
    // toggles the alert flag can never clear the account's address.
    let email_update = match req.email.as_deref() {
        Some(raw) => match normalize_account_email(raw) {
            Ok(value) => Some(value),
            Err(error_message) => {
                return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                    "error": error_message
                })));
            }
        },
        None => None,
    };

    let db = data.db.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(email) = &email_update {
        match db.execute(
            "UPDATE users SET email = ?1 WHERE userID = ?2",
            params![email.as_deref(), claims.user_id],
        ) {
            Ok(rows_affected) if rows_affected > 0 => {
                info!(
                    user_id = claims.user_id,
                    has_email = email.is_some(),
                    "Account email updated"
                );
            }
            Ok(_) => {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "User not found"
                })));
            }
            Err(e) => {
                error!(user_id = claims.user_id, error = %e, "Failed to update account email");
                return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Failed to update account"
                })));
            }
        }
    }

    if let Some(enabled) = req.notify_new_location {
        if let Err(e) = crate::location_alert::set_notify_new_location(&db, claims.user_id, enabled)
        {
            error!(user_id = claims.user_id, error = %e, "Failed to update account settings");
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update account"
            })));
        }
        info!(
            user_id = claims.user_id,
            notify_new_location = enabled,
            "Account settings updated"
        );
    }

    let stored_email: Option<String> = db
        .query_row(
            "SELECT email FROM users WHERE userID = ?1",
            params![claims.user_id],
            |row| row.get(0),
        )
        .unwrap_or(None);

    match crate::location_alert::get_notify_new_location(&db, claims.user_id) {
        Ok(Some(notify_new_location)) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "message": "Account updated successfully",
            "email": stored_email,
            "notify_new_location": notify_new_location,
        }))),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "User not found"
        }))),
        Err(e) => {
            error!(user_id = claims.user_id, error = %e, "Failed to read account settings");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update account"
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::middleware::jwt_validator;
    use crate::testing::{make_test_state, TEST_PASSWORD};
    use actix_web::{test, App};
    use actix_web_httpauth::middleware::HttpAuthentication;
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
                            .route("/me", web::get().to(get_current_user))
                            .route("/me", web::patch().to(update_current_user)),
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

    // --- account email (RUS-11) ---

    /// The account's stored address, straight from the column the alert reads.
    fn stored_email(state: &actix_web::web::Data<AppState>, username: &str) -> Option<String> {
        let db = state.db.lock().unwrap();
        db.query_row(
            "SELECT email FROM users WHERE username = ?1",
            params![username],
            |row| row.get(0),
        )
        .unwrap()
    }

    /// PATCH /api/me with the given JSON body.
    async fn patch_me(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        token: &str,
        body: Value,
    ) -> actix_web::dev::ServiceResponse {
        let req = test::TestRequest::patch()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(body)
            .to_request();
        test::call_service(app, req).await
    }

    // An existing signup flow that sends no email still works, and leaves the
    // account with no address.
    #[actix_web::test]
    async fn register_without_email_stores_null() {
        let state = make_test_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        assert_eq!(stored_email(&state, "alice"), None);
    }

    // An address given at registration is normalized on the way in.
    #[actix_web::test]
    async fn register_with_email_stores_it_normalized() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({
                "username": "alice",
                "password": TEST_PASSWORD,
                "email": "  Alice@Example.COM "
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        assert_eq!(
            stored_email(&state, "alice"),
            Some("alice@example.com".to_string())
        );
    }

    #[actix_web::test]
    async fn register_with_malformed_email_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::post()
            .uri("/api/register")
            .set_json(serde_json::json!({
                "username": "alice",
                "password": TEST_PASSWORD,
                "email": "not-an-address"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    // The account can set its address after the fact, and read it back.
    #[actix_web::test]
    async fn patch_me_sets_the_account_email() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();

        let resp = patch_me(
            &app,
            token,
            serde_json::json!({"email": "Alice@Example.com"}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        assert_eq!(
            stored_email(&state, "alice"),
            Some("alice@example.com".to_string())
        );

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["email"], "alice@example.com");
    }

    // Clearing it stores NULL, not an empty string, so the alert falls back to
    // the operator mailbox rather than trying to mail "".
    #[actix_web::test]
    async fn patch_me_with_blank_email_stores_null() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();

        patch_me(
            &app,
            token,
            serde_json::json!({"email": "alice@example.com"}),
        )
        .await;
        let resp = patch_me(&app, token, serde_json::json!({"email": "   "})).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(stored_email(&state, "alice"), None);

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert!(me["email"].is_null());
    }

    #[actix_web::test]
    async fn patch_me_with_malformed_email_returns_400() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();

        for bad in ["alice", "@example.com", "alice@"] {
            let resp = patch_me(&app, token, serde_json::json!({"email": bad})).await;
            assert_eq!(resp.status(), 400, "expected {bad:?} to be rejected");
        }
        assert_eq!(stored_email(&state, "alice"), None, "nothing was stored");
    }

    #[actix_web::test]
    async fn patch_me_without_token_returns_401() {
        let state = make_test_state();
        let app = setup_app!(state);
        let req = test::TestRequest::patch()
            .uri("/api/me")
            .set_json(serde_json::json!({"email": "alice@example.com"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
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

    // --- new-location alert opt-out (RUS-15) ---

    /// The account's stored preference, straight from the column the alert reads.
    fn stored_notify(state: &actix_web::web::Data<AppState>, username: &str) -> bool {
        let db = state.db.lock().unwrap();
        let user_id: i64 = db
            .query_row(
                "SELECT userID FROM users WHERE username = ?1",
                params![username],
                |r| r.get(0),
            )
            .unwrap();
        crate::location_alert::get_notify_new_location(&db, user_id)
            .unwrap()
            .unwrap()
    }

    /// PATCH /api/me with the given JSON body.
    #[actix_web::test]
    async fn get_me_reports_the_stored_preference() {
        let state = make_test_state();
        let app = setup_app!(state);
        let token = do_register(&app, "alice").await["token"]
            .as_str()
            .unwrap()
            .to_string();

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["notify_new_location"], true, "alerts default to on");

        patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let me: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(me["notify_new_location"], false);
    }

    // An explicit false is a real opt-out and has to reach the column; sending
    // true again turns the alerts back on.
    #[actix_web::test]
    async fn patch_me_persists_both_directions() {
        let state = make_test_state();
        let app = setup_app!(state);
        let token = do_register(&app, "alice").await["token"]
            .as_str()
            .unwrap()
            .to_string();

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["notify_new_location"], false);
        assert!(!stored_notify(&state, "alice"));

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": true}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        assert!(stored_notify(&state, "alice"));
    }

    // A missing key means "not submitted", so the stored value stands rather
    // than silently reverting to the default.
    #[actix_web::test]
    async fn patch_me_without_the_key_leaves_the_value_unchanged() {
        let state = make_test_state();
        let app = setup_app!(state);
        let token = do_register(&app, "alice").await["token"]
            .as_str()
            .unwrap()
            .to_string();
        patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;

        for body in [serde_json::json!({}), serde_json::json!({"unrelated": 1})] {
            let resp = patch_me(&app, &token, body).await;
            assert_eq!(resp.status(), 200);
            assert!(
                !stored_notify(&state, "alice"),
                "an absent key must not re-enable alerts"
            );
        }
    }

    // The two account fields are independent, and the dashboard submits only
    // what the user changed, so a save of one must never disturb the other.
    #[actix_web::test]
    async fn patch_me_leaves_the_field_it_was_not_sent_alone() {
        let state = make_test_state();
        let app = setup_app!(state);
        let body = do_register(&app, "alice").await;
        let token = body["token"].as_str().unwrap();
        patch_me(
            &app,
            token,
            serde_json::json!({"email": "alice@example.com"}),
        )
        .await;

        // Turning the alert off keeps the address, in the store and in the
        // response the dashboard repaints its controls from.
        let resp = patch_me(
            &app,
            token,
            serde_json::json!({"notify_new_location": false}),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let echoed: Value = test::read_body_json(resp).await;
        assert_eq!(echoed["email"], "alice@example.com");
        assert_eq!(
            stored_email(&state, "alice"),
            Some("alice@example.com".to_string()),
            "a toggle-only save must not clear the address"
        );

        // Changing the address keeps the opt-out.
        let resp = patch_me(&app, token, serde_json::json!({"email": "new@example.com"})).await;
        assert_eq!(resp.status(), 200);
        let echoed: Value = test::read_body_json(resp).await;
        assert_eq!(echoed["notify_new_location"], false);
        assert!(
            !stored_notify(&state, "alice"),
            "an address-only save must not re-enable the alert"
        );
    }

    // The type is validated, never coerced: a string is a bad request and
    // changes nothing.
    #[actix_web::test]
    async fn patch_me_rejects_a_non_boolean() {
        let state = make_test_state();
        let app = setup_app!(state);
        let token = do_register(&app, "alice").await["token"]
            .as_str()
            .unwrap()
            .to_string();

        let resp = patch_me(
            &app,
            &token,
            serde_json::json!({"notify_new_location": "false"}),
        )
        .await;
        assert_eq!(resp.status(), 400);
        assert!(stored_notify(&state, "alice"));
    }

    // The write targets the session's account. An id in the body is ignored, so
    // one account cannot flip another's setting.
    #[actix_web::test]
    async fn patch_me_ignores_a_user_id_in_the_body() {
        let state = make_test_state();
        let app = setup_app!(state);
        let alice = do_register(&app, "alice").await["token"]
            .as_str()
            .unwrap()
            .to_string();
        do_register(&app, "bob").await;
        let bob_id: i64 = {
            let db = state.db.lock().unwrap();
            db.query_row("SELECT userID FROM users WHERE username = 'bob'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };

        let resp = patch_me(
            &app,
            &alice,
            serde_json::json!({
                "notify_new_location": false,
                "user_id": bob_id,
                "userID": bob_id,
                "username": "bob",
            }),
        )
        .await;
        assert_eq!(resp.status(), 200);
        assert!(!stored_notify(&state, "alice"), "alice opted out");
        assert!(stored_notify(&state, "bob"), "bob is untouched");
    }

    // ── RUS-19: the approval gate, at the real /api/login route ──────────────
    //
    // The pure decision is covered in `login_approval`; these drive it through
    // the route that actually mints a standalone session, because the property
    // that matters is "no token comes back", not "the predicate returned true".

    /// A state whose gate is on and whose approval mail is deliverable by
    /// configuration. The relay points at a closed port, so a held sign-in
    /// writes its row and then fails the send immediately rather than hanging
    /// the test on a network timeout.
    fn gated_state() -> web::Data<AppState> {
        let mut config = crate::testing::test_config();
        config.mail = crate::config::MailConfig {
            smtp_host: Some("127.0.0.1".to_string()),
            smtp_port: Some(1),
            smtp_from_email: Some("no-reply@example.com".to_string()),
            login_approval_enabled: true,
            ..crate::config::MailConfig::default()
        };
        web::Data::new(AppState::new(config).unwrap())
    }

    /// Trust the loopback so `X-IPCountry` is honoured at all (RUS-12).
    fn trust_loopback() {
        crate::location_alert::init_trusted_proxies(vec![
            "127.0.0.0/8".parse().unwrap(),
            "::1/128".parse().unwrap(),
        ]);
    }

    fn set_account(
        state: &web::Data<AppState>,
        username: &str,
        country: Option<&str>,
        notify: bool,
    ) {
        let db = state.db.lock().unwrap();
        db.execute(
            "UPDATE users SET last_login_country = ?1, email = 'alice@example.com',
             notify_new_location = ?2 WHERE username = ?3",
            params![country, notify as i64, username],
        )
        .unwrap();
    }

    fn stored_country(state: &web::Data<AppState>, username: &str) -> Option<String> {
        let db = state.db.lock().unwrap();
        db.query_row(
            "SELECT last_login_country FROM users WHERE username = ?1",
            params![username],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn pending_rows(state: &web::Data<AppState>) -> i64 {
        let db = state.db.lock().unwrap();
        db.query_row("SELECT COUNT(*) FROM pending_login_approvals", [], |r| {
            r.get(0)
        })
        .unwrap()
    }

    /// Sign in as `username` from `country`, or from nowhere when it is `None`.
    async fn login_from(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
        username: &str,
        country: Option<&str>,
    ) -> actix_web::dev::ServiceResponse {
        let mut req = test::TestRequest::post()
            .uri("/api/login")
            .set_json(serde_json::json!({"username": username, "password": TEST_PASSWORD}));
        if let Some(code) = country {
            req = req
                .peer_addr("127.0.0.1:34567".parse().unwrap())
                .insert_header(("X-IPCountry", code));
        }
        test::call_service(app, req.to_request()).await
    }

    // Property 1. Getting this wrong means the first account ever created can
    // never sign in.
    #[actix_web::test]
    async fn login_is_never_gated_on_a_first_ever_sign_in() {
        trust_loopback();
        let state = gated_state();
        let app = setup_app!(state);
        // Registered with no country, so there is no baseline to differ from.
        do_register(&app, "alice").await;
        set_account(&state, "alice", None, true);

        let resp = login_from(&app, "alice", Some("DE")).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
        assert_eq!(pending_rows(&state), 0);
    }

    // Property 2. Getting this wrong bricks every deployment without the
    // geoblock edge, since off it no country ever resolves.
    #[actix_web::test]
    async fn login_is_never_gated_when_no_country_resolves() {
        trust_loopback();
        let state = gated_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("US"), true);

        let resp = login_from(&app, "alice", None).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
        assert_eq!(pending_rows(&state), 0);
    }

    // Property 3. Off by default, so a deployment that says nothing behaves
    // exactly as it did before RUS-19.
    #[actix_web::test]
    async fn login_is_never_gated_while_the_kill_switch_is_off() {
        trust_loopback();
        let state = make_test_state();
        assert!(!state.config.mail.login_approval_enabled);
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("US"), true);

        let resp = login_from(&app, "alice", Some("DE")).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
        assert_eq!(pending_rows(&state), 0);
    }

    #[actix_web::test]
    async fn login_from_a_known_country_is_never_gated() {
        trust_loopback();
        let state = gated_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("de"), true);

        let resp = login_from(&app, "alice", Some("DE")).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(pending_rows(&state), 0);
    }

    #[actix_web::test]
    async fn login_from_a_new_country_yields_no_session() {
        trust_loopback();
        let state = gated_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("US"), true);

        let resp = login_from(&app, "alice", Some("DE")).await;
        // The fixture's relay is unreachable, so the hold is written and the
        // send then fails: closed, with nothing issued either way.
        assert_eq!(resp.status(), 500);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_null(), "a held sign-in issues nothing");
        assert_eq!(
            pending_rows(&state),
            1,
            "the hold is recorded before the mail"
        );
        assert_eq!(
            stored_country(&state, "alice").as_deref(),
            Some("US"),
            "an unapproved attempt must not make its country familiar"
        );
    }

    // The alert opt-out is written from a session, so honouring it here would
    // let anyone holding a session switch off the control that defends the
    // account against them.
    #[actix_web::test]
    async fn the_alert_opt_out_does_not_disable_the_login_gate() {
        trust_loopback();
        let state = gated_state();
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("US"), false);

        let resp = login_from(&app, "alice", Some("DE")).await;
        assert_ne!(resp.status(), 200);
        assert_eq!(pending_rows(&state), 1);
    }

    // A gate whose link cannot be delivered is a lockout, so an unconfigured
    // transport degrades to the RUS-7 alert rather than holding.
    #[actix_web::test]
    async fn login_from_a_new_country_completes_when_no_link_can_be_delivered() {
        trust_loopback();
        let mut config = crate::testing::test_config();
        config.mail = crate::config::MailConfig {
            login_approval_enabled: true,
            ..crate::config::MailConfig::default()
        };
        let state = web::Data::new(AppState::new(config).unwrap());
        let app = setup_app!(state);
        do_register(&app, "alice").await;
        set_account(&state, "alice", Some("US"), true);

        let resp = login_from(&app, "alice", Some("DE")).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(body["token"].is_string());
        assert_eq!(pending_rows(&state), 0);
    }
}
