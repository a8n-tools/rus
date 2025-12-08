use actix_web::{web, HttpRequest, HttpResponse, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use rusqlite::params;

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
    // Check if registration is allowed
    if !data.config.allow_registration {
        // Check if this would be the first user (setup)
        let db = data.db.lock().unwrap();
        let user_count: i64 = db
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap_or(0);

        // Allow first user registration for setup, block all others
        if user_count > 0 {
            return Ok(HttpResponse::Forbidden().json(serde_json::json!({
                "error": "New user registration is disabled. Please contact the administrator."
            })));
        }
    }

    // Validate input
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

    // Validate password complexity
    if let Err(error_message) = validate_password(&req.password) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })));
    }

    // Hash password
    let hashed_password = match hash(&req.password, DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to hash password"
            })));
        }
    };

    // Insert user into database
    let db = data.db.lock().unwrap();

    // Check if this is the first user (will be admin)
    let user_count: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

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

                    Ok(HttpResponse::Created().json(AuthResponse {
                        token,
                        refresh_token,
                        username: req.username.clone(),
                    }))
                }
                Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Failed to create token"
                }))),
            }
        }
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                Ok(HttpResponse::Conflict().json(serde_json::json!({
                    "error": "Username already exists"
                })))
            } else {
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
    let db = data.db.lock().unwrap();

    // Check for account lockout BEFORE any other database operations
    // This prevents timing attacks that could reveal if a username exists
    if is_account_locked(
        &db,
        &req.username,
        data.config.account_lockout_attempts,
        data.config.account_lockout_duration_minutes,
    ) {
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
            // Verify password
            match verify(&req.password, &hashed_password) {
                Ok(true) => {
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

                            Ok(HttpResponse::Ok().json(AuthResponse {
                                token,
                                refresh_token,
                                username,
                            }))
                        }
                        Err(_) => Ok(HttpResponse::InternalServerError().json(
                            serde_json::json!({
                                "error": "Failed to create token"
                            }),
                        )),
                    }
                }
                Ok(false) => {
                    // Record failed login attempt (wrong password)
                    record_login_attempt(&db, &req.username, false);
                    Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "Invalid credentials"
                    })))
                }
                Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Authentication error"
                }))),
            }
        }
        Err(_) => {
            // Record failed login attempt (user not found)
            record_login_attempt(&db, &req.username, false);
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
    let db = data.db.lock().unwrap();

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

            Ok(HttpResponse::Ok().json(RefreshResponse {
                token,
                refresh_token: new_refresh_token,
            }))
        }
        Err(_) => Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid or expired refresh token"
        }))),
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
