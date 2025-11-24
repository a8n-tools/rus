use actix_web::{web, App, HttpResponse, HttpServer, Result, middleware, HttpRequest, HttpMessage};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use actix_web_httpauth::middleware::HttpAuthentication;
use serde::{Deserialize, Serialize};
use rusqlite::{Connection, params};
use std::sync::Mutex;
use std::env;
use rand::Rng;
use bcrypt::{hash, verify, DEFAULT_COST};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use chrono::{Utc, Duration};
use url::Url;
use qrcode::QrCode;
use qrcode::render::svg;
use image::{DynamicImage, Luma};

// Configuration structure
#[derive(Clone, Debug)]
struct Config {
    jwt_secret: String,
    jwt_expiry_hours: i64,
    refresh_token_expiry_days: i64,
    max_url_length: usize,
    account_lockout_attempts: i32,
    account_lockout_duration_minutes: i64,
    click_retention_days: i64,
    host_url: String,
    db_path: String,
    host: String,
    port: u16,
}

impl Config {
    fn from_env() -> Self {
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            eprintln!("WARNING: JWT_SECRET not set in environment, using default (insecure)");
            "your-secret-key-change-this-in-production".to_string()
        });

        let jwt_expiry_hours = env::var("JWT_EXPIRY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let refresh_token_expiry_days = env::var("REFRESH_TOKEN_EXPIRY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7);

        let max_url_length = env::var("MAX_URL_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2048);

        let account_lockout_attempts = env::var("ACCOUNT_LOCKOUT_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let account_lockout_duration_minutes = env::var("ACCOUNT_LOCKOUT_DURATION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let click_retention_days = env::var("CLICK_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let host_url = env::var("HOST_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        let db_path = env::var("DB_PATH")
            .unwrap_or_else(|_| "./data/rus.db".to_string());

        let host = env::var("HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        Config {
            jwt_secret,
            jwt_expiry_hours,
            refresh_token_expiry_days,
            max_url_length,
            account_lockout_attempts,
            account_lockout_duration_minutes,
            click_retention_days,
            host_url,
            db_path,
            host,
            port,
        }
    }
}

// Helper function to get JWT secret from environment (used by decode_jwt)
fn get_jwt_secret() -> String {
    env::var("JWT_SECRET").unwrap_or_else(|_| {
        eprintln!("WARNING: JWT_SECRET not set in environment, using default (insecure)");
        "your-secret-key-change-this-in-production".to_string()
    })
}

// Data structures
#[derive(Serialize, Deserialize)]
struct ShortenRequest {
    url: String,
}

#[derive(Serialize, Deserialize)]
struct ShortenResponse {
    short_code: String,
    short_url: String,
    original_url: String,
}

#[derive(Clone, Serialize)]
struct UrlEntry {
    original_url: String,
    short_code: String,
    name: Option<String>,
    clicks: u64,
}

#[derive(Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct AuthResponse {
    token: String,
    refresh_token: String,
    username: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateUrlNameRequest {
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct RefreshResponse {
    token: String,
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct ClickHistoryEntry {
    clicked_at: String,
}

#[derive(Serialize, Deserialize)]
struct ClickStats {
    total_clicks: u64,
    history: Vec<ClickHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,      // username
    user_id: i64,     // user ID
    is_admin: bool,   // admin flag
    exp: usize,       // expiration time
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_seconds: u64,
}

#[derive(Serialize)]
struct ConfigResponse {
    host_url: String,
    max_url_length: usize,
}

#[derive(Serialize)]
struct SetupCheckResponse {
    setup_required: bool,
}

#[derive(Serialize)]
struct UserInfo {
    user_id: i64,
    username: String,
    is_admin: bool,
    created_at: String,
    url_count: i64,
}

#[derive(Serialize)]
struct CurrentUserResponse {
    user_id: i64,
    username: String,
    is_admin: bool,
}

#[derive(Serialize)]
struct AdminStatsResponse {
    total_users: i64,
    total_urls: i64,
    total_clicks: i64,
}

// Application state
struct AppState {
    db: Mutex<Connection>,
    config: Config,
    start_time: std::time::Instant,
}

impl AppState {
    fn new(config: Config) -> rusqlite::Result<Self> {
        // Create data directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&config.db_path)?;

        // Initialize database schema
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                userID INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password TEXT NOT NULL,
                is_admin INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS urls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                original_url TEXT NOT NULL,
                short_code TEXT NOT NULL UNIQUE,
                name TEXT,
                clicks INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS click_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url_id INTEGER NOT NULL,
                clicked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (url_id) REFERENCES urls(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS refresh_tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                token TEXT NOT NULL UNIQUE,
                expires_at DATETIME NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(userID) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS login_attempts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                attempted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                success INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_short_code ON urls(short_code);
            CREATE INDEX IF NOT EXISTS idx_user_id ON urls(user_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_url_id ON click_history(url_id);
            CREATE INDEX IF NOT EXISTS idx_click_history_clicked_at ON click_history(clicked_at);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token ON refresh_tokens(token);
            CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
            CREATE INDEX IF NOT EXISTS idx_login_attempts_username ON login_attempts(username);
            CREATE INDEX IF NOT EXISTS idx_login_attempts_attempted_at ON login_attempts(attempted_at);
            "
        )?;

        Ok(AppState {
            db: Mutex::new(conn),
            config,
            start_time: std::time::Instant::now(),
        })
    }
}

// Generate a random short code
fn generate_short_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const CODE_LENGTH: usize = 6;

    let mut rng = rand::thread_rng();
    (0..CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

// Validate password complexity
fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters long".to_string());
    }

    if !password.chars().any(|c| c.is_uppercase()) {
        return Err("Password must contain at least one uppercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_numeric()) {
        return Err("Password must contain at least one number".to_string());
    }

    if !password.chars().any(|c| !c.is_alphanumeric()) {
        return Err("Password must contain at least one special character".to_string());
    }

    Ok(())
}

// Check if account is locked due to too many failed login attempts
fn is_account_locked(db: &Connection, username: &str, max_attempts: i32, lockout_minutes: i64) -> bool {
    let cutoff = Utc::now() - Duration::minutes(lockout_minutes);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let failed_attempts: i32 = db.query_row(
        "SELECT COUNT(*) FROM login_attempts WHERE username = ?1 AND success = 0 AND attempted_at > ?2",
        params![username, cutoff_str],
        |row| row.get(0),
    ).unwrap_or(0);

    failed_attempts >= max_attempts
}

// Record a login attempt (success or failure) for tracking
fn record_login_attempt(db: &Connection, username: &str, success: bool) {
    let _ = db.execute(
        "INSERT INTO login_attempts (username, success) VALUES (?1, ?2)",
        params![username, success as i32],
    );
}

// Cleanup old click history records
fn cleanup_old_clicks(db: &Connection, retention_days: i64) {
    let cutoff = Utc::now() - Duration::days(retention_days);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let _ = db.execute(
        "DELETE FROM click_history WHERE clicked_at < ?1",
        params![cutoff_str],
    );
}

// Validate URL for shortening
fn validate_url(url_str: &str, max_length: usize) -> Result<(), String> {
    if url_str.len() > max_length {
        return Err(format!("URL exceeds maximum length of {} characters", max_length));
    }

    let parsed = Url::parse(url_str).map_err(|_| "Invalid URL format".to_string())?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }

    // Block dangerous patterns
    let dangerous_patterns = [
        "javascript:",
        "data:",
        "file:",
        "vbscript:",
        "about:",
    ];

    let url_lower = url_str.to_lowercase();
    for pattern in &dangerous_patterns {
        if url_lower.contains(pattern) {
            return Err(format!("URL contains blocked pattern: {}", pattern));
        }
    }

    Ok(())
}

// Generate QR code as PNG
fn generate_qr_code_png(url: &str) -> Result<Vec<u8>, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let image = code.render::<Luma<u8>>()
        .min_dimensions(400, 400)
        .build();

    // Convert to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    DynamicImage::ImageLuma8(image)
        .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}

// Generate QR code as SVG
fn generate_qr_code_svg(url: &str) -> Result<String, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let svg_string = code.render()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_string)
}

// Generate a cryptographically secure refresh token
fn generate_refresh_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

// JWT helper functions
fn create_jwt(username: &str, user_id: i64, is_admin: bool, secret: &str, expiry_hours: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(expiry_hours))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        user_id,
        is_admin,
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
}

fn decode_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

// JWT validator middleware
async fn jwt_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}

// Admin validator middleware (requires valid JWT with admin flag)
async fn admin_validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let token = credentials.token();

    // Get the secret from app state
    let secret = req
        .app_data::<web::Data<AppState>>()
        .map(|state| state.config.jwt_secret.clone())
        .unwrap_or_else(get_jwt_secret);

    match decode_jwt(token, &secret) {
        Ok(claims) => {
            if !claims.is_admin {
                return Err((
                    actix_web::error::ErrorForbidden("Admin access required"),
                    req,
                ));
            }
            req.extensions_mut().insert(claims);
            Ok(req)
        }
        Err(_) => Err((
            actix_web::error::ErrorUnauthorized("Invalid token"),
            req,
        )),
    }
}

// Extract claims from request
fn get_claims(req: &HttpRequest) -> Option<Claims> {
    req.extensions().get::<Claims>().cloned()
}

// Authentication endpoints
async fn register(
    data: web::Data<AppState>,
    req: web::Json<RegisterRequest>,
) -> Result<HttpResponse> {
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
    let user_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM users",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let is_admin = user_count == 0;

    match db.execute(
        "INSERT INTO users (username, password, is_admin) VALUES (?1, ?2, ?3)",
        params![&req.username, &hashed_password, is_admin as i32],
    ) {
        Ok(_) => {
            // Get the user ID
            let user_id: i64 = db.last_insert_rowid();

            // Create JWT token
            match create_jwt(&req.username, user_id, is_admin, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                Ok(token) => {
                    // Create refresh token
                    let refresh_token = generate_refresh_token();
                    let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
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

async fn login(
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
    let mut stmt = match db.prepare("SELECT userID, username, password, is_admin FROM users WHERE username = ?1") {
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
                    match create_jwt(&username, user_id, is_admin, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                        Ok(token) => {
                            // Create refresh token
                            let refresh_token = generate_refresh_token();
                            let expires_at = Utc::now() + Duration::days(data.config.refresh_token_expiry_days);
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
                        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": "Failed to create token"
                        }))),
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

// Token refresh endpoint
async fn refresh_token(
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
            let _ = db.execute("DELETE FROM refresh_tokens WHERE id = ?1", params![token_id]);

            // Create new JWT token
            let token = match create_jwt(&username, user_id, is_admin, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
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

// Protected API endpoint to shorten a URL
async fn shorten_url(
    data: web::Data<AppState>,
    req_payload: web::Json<ShortenRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Get user claims from JWT
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    // Validate URL
    if req_payload.url.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "URL cannot be empty"
        })));
    }

    // Comprehensive URL validation
    if let Err(error_message) = validate_url(&req_payload.url, data.config.max_url_length) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })));
    }

    let db = data.db.lock().unwrap();

    // Check if URL is already shortened by this user
    let mut stmt = db.prepare("SELECT short_code FROM urls WHERE user_id = ?1 AND original_url = ?2")
        .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    if let Ok(short_code) = stmt.query_row(
        params![claims.user_id, &req_payload.url],
        |row| row.get::<_, String>(0),
    ) {
        return Ok(HttpResponse::Ok().json(ShortenResponse {
            short_code: short_code.clone(),
            short_url: format!("{}/{}", data.config.host_url, short_code),
            original_url: req_payload.url.clone(),
        }));
    }

    // Generate a unique short code
    let mut short_code = generate_short_code();
    loop {
        let exists: bool = db.query_row(
            "SELECT COUNT(*) FROM urls WHERE short_code = ?1",
            params![&short_code],
            |row| row.get(0),
        )
        .map(|count: i64| count > 0)
        .unwrap_or(false);

        if !exists {
            break;
        }
        short_code = generate_short_code();
    }

    // Insert URL into database
    match db.execute(
        "INSERT INTO urls (user_id, original_url, short_code) VALUES (?1, ?2, ?3)",
        params![claims.user_id, &req_payload.url, &short_code],
    ) {
        Ok(_) => Ok(HttpResponse::Ok().json(ShortenResponse {
            short_code: short_code.clone(),
            short_url: format!("{}/{}", data.config.host_url, short_code),
            original_url: req_payload.url.clone(),
        })),
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to create short URL"
        }))),
    }
}

// Public endpoint to redirect to the original URL
async fn redirect_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    // Get URL ID and original URL
    let result: rusqlite::Result<(i64, String)> = db.query_row(
        "SELECT id, original_url FROM urls WHERE short_code = ?1",
        params![code.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((url_id, original_url)) => {
            // Increment click count (legacy counter)
            let _ = db.execute(
                "UPDATE urls SET clicks = clicks + 1 WHERE id = ?1",
                params![url_id],
            );

            // Record click in history
            let _ = db.execute(
                "INSERT INTO click_history (url_id) VALUES (?1)",
                params![url_id],
            );

            // Cleanup old clicks periodically (1% chance)
            if rand::thread_rng().gen_range(0..100) == 0 {
                cleanup_old_clicks(&db, data.config.click_retention_days);
            }

            Ok(HttpResponse::Found()
                .append_header(("Location", original_url))
                .finish())
        }
        Err(_) => {
            // Serve the 404 page
            let html = include_str!("../static/404.html");
            Ok(HttpResponse::NotFound()
                .content_type("text/html; charset=utf-8")
                .body(html))
        }
    }
}

// Protected API endpoint to get URL statistics
async fn get_stats(
    data: web::Data<AppState>,
    code: web::Path<String>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    // Get user claims from JWT
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Get URL entry for this user
    let result: rusqlite::Result<UrlEntry> = db.query_row(
        "SELECT original_url, short_code, name, clicks FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
        |row| {
            Ok(UrlEntry {
                original_url: row.get(0)?,
                short_code: row.get(1)?,
                name: row.get(2)?,
                clicks: row.get(3)?,
            })
        },
    );

    match result {
        Ok(entry) => Ok(HttpResponse::Ok().json(entry)),
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found or not owned by you"
        }))),
    }
}

// Protected endpoint to get all URLs for the current user
async fn get_user_urls(
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

    let db = data.db.lock().unwrap();

    let mut stmt = db.prepare(
        "SELECT original_url, short_code, name, clicks FROM urls WHERE user_id = ?1 ORDER BY created_at DESC"
    ).map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let urls: Vec<UrlEntry> = stmt.query_map(params![claims.user_id], |row| {
        Ok(UrlEntry {
            original_url: row.get(0)?,
            short_code: row.get(1)?,
            name: row.get(2)?,
            clicks: row.get(3)?,
        })
    })
    .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(HttpResponse::Ok().json(urls))
}

// Protected endpoint to delete a URL
async fn delete_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
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

    let db = data.db.lock().unwrap();

    // Delete the URL only if it belongs to the current user
    match db.execute(
        "DELETE FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "URL deleted successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Short URL not found or not owned by you"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to delete URL"
        }))),
    }
}

// Protected endpoint to update URL name
async fn update_url_name(
    data: web::Data<AppState>,
    code: web::Path<String>,
    req_payload: web::Json<UpdateUrlNameRequest>,
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

    let db = data.db.lock().unwrap();

    // Update the URL name only if it belongs to the current user
    match db.execute(
        "UPDATE urls SET name = ?1 WHERE short_code = ?2 AND user_id = ?3",
        params![req_payload.name.as_deref(), code.as_str(), claims.user_id],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "URL name updated successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Short URL not found or not owned by you"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to update URL name"
        }))),
    }
}

// Protected endpoint to get click history
async fn get_click_history(
    data: web::Data<AppState>,
    code: web::Path<String>,
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

    let db = data.db.lock().unwrap();

    // First verify ownership
    let url_id: rusqlite::Result<i64> = db.query_row(
        "SELECT id FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
        |row| row.get(0),
    );

    let url_id = match url_id {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Short URL not found or not owned by you"
            })));
        }
    };

    // Get total clicks from counter
    let total_clicks: u64 = db.query_row(
        "SELECT clicks FROM urls WHERE id = ?1",
        params![url_id],
        |row| row.get(0),
    ).unwrap_or(0);

    // Get click history (limited to recent 1000)
    let mut stmt = db.prepare(
        "SELECT clicked_at FROM click_history WHERE url_id = ?1 ORDER BY clicked_at DESC LIMIT 1000"
    ).map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let history: Vec<ClickHistoryEntry> = stmt.query_map(params![url_id], |row| {
        Ok(ClickHistoryEntry {
            clicked_at: row.get(0)?,
        })
    })
    .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(HttpResponse::Ok().json(ClickStats {
        total_clicks,
        history,
    }))
}

// Protected endpoint to generate and download QR codes
async fn get_qr_code(
    data: web::Data<AppState>,
    path: web::Path<(String, String)>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (code, format) = path.into_inner();

    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Verify ownership
    let exists: bool = db.query_row(
        "SELECT COUNT(*) FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![&code, claims.user_id],
        |row| row.get::<_, i64>(0),
    ).map(|count| count > 0).unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found or not owned by you"
        })));
    }

    // Get the actual host from the request
    let host = http_req
        .connection_info()
        .host()
        .to_string();

    let scheme = if http_req.connection_info().scheme() == "https" {
        "https"
    } else {
        "http"
    };

    let full_url = format!("{}://{}/{}", scheme, host, code);
    drop(db); // Release lock before heavy computation

    match format.as_str() {
        "png" => {
            match generate_qr_code_png(&full_url) {
                Ok(png_bytes) => Ok(HttpResponse::Ok()
                    .content_type("image/png")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}.png\"", code)))
                    .body(png_bytes)),
                Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": format!("Failed to generate QR code: {}", e)
                }))),
            }
        }
        "svg" => {
            match generate_qr_code_svg(&full_url) {
                Ok(svg_string) => Ok(HttpResponse::Ok()
                    .content_type("image/svg+xml")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}.svg\"", code)))
                    .body(svg_string)),
                Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": format!("Failed to generate QR code: {}", e)
                }))),
            }
        }
        _ => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid format. Use 'png' or 'svg'"
        }))),
    }
}

// Serve static HTML pages
async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html")))
}

async fn login_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/login.html")))
}

async fn signup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/signup.html")))
}

async fn dashboard_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/dashboard.html")))
}

async fn setup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/setup.html")))
}

async fn admin_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/admin.html")))
}

async fn serve_css() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../static/styles.css")))
}

async fn serve_auth_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../static/auth.js")))
}

// Health check endpoint for monitoring and Docker health checks
async fn health_check(data: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = data.start_time.elapsed().as_secs();

    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    }))
}

// Public config endpoint for frontend
async fn get_config(data: web::Data<AppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ConfigResponse {
        host_url: data.config.host_url.clone(),
        max_url_length: data.config.max_url_length,
    }))
}

// Check if initial setup is required (no users exist)
async fn check_setup_required(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let user_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM users",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok(HttpResponse::Ok().json(SetupCheckResponse {
        setup_required: user_count == 0,
    }))
}

// Get current user info
async fn get_current_user(
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

    Ok(HttpResponse::Ok().json(CurrentUserResponse {
        user_id: claims.user_id,
        username: claims.sub,
        is_admin: claims.is_admin,
    }))
}

// Admin endpoint to list all users
async fn admin_list_users(
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let mut stmt = db.prepare(
        "SELECT u.userID, u.username, u.is_admin, u.created_at,
                (SELECT COUNT(*) FROM urls WHERE user_id = u.userID) as url_count
         FROM users u
         ORDER BY u.created_at DESC"
    ).map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let users: Vec<UserInfo> = stmt.query_map([], |row| {
        Ok(UserInfo {
            user_id: row.get(0)?,
            username: row.get(1)?,
            is_admin: row.get::<_, i32>(2)? != 0,
            created_at: row.get(3)?,
            url_count: row.get(4)?,
        })
    })
    .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(HttpResponse::Ok().json(users))
}

// Admin endpoint to delete a user
async fn admin_delete_user(
    data: web::Data<AppState>,
    user_id: web::Path<i64>,
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

    // Prevent admin from deleting themselves
    if claims.user_id == *user_id {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Cannot delete your own account"
        })));
    }

    let db = data.db.lock().unwrap();

    // Delete the user (CASCADE will handle related records)
    match db.execute(
        "DELETE FROM users WHERE userID = ?1",
        params![*user_id],
    ) {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "message": "User deleted successfully"
                })))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "User not found"
                })))
            }
        }
        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to delete user"
        }))),
    }
}

// Admin endpoint to get system statistics
async fn admin_get_stats(
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let total_users: i64 = db.query_row(
        "SELECT COUNT(*) FROM users",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let total_urls: i64 = db.query_row(
        "SELECT COUNT(*) FROM urls",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let total_clicks: i64 = db.query_row(
        "SELECT SUM(clicks) FROM urls",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok(HttpResponse::Ok().json(AdminStatsResponse {
        total_users,
        total_urls,
        total_clicks,
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Load configuration from environment
    let config = Config::from_env();

    // Print startup banner with configuration
    println!("========================================");
    println!("  Rust URL Shortener - Configuration");
    println!("========================================");
    println!("Host: {}", config.host);
    println!("Port: {}", config.port);
    println!("Host URL: {}", config.host_url);
    println!("Database Path: {}", config.db_path);
    println!("JWT Expiry: {} hours", config.jwt_expiry_hours);
    println!("Refresh Token Expiry: {} days", config.refresh_token_expiry_days);
    println!("Max URL Length: {}", config.max_url_length);
    println!("Account Lockout Attempts: {}", config.account_lockout_attempts);
    println!("Account Lockout Duration: {} minutes", config.account_lockout_duration_minutes);
    println!("Click Retention: {} days", config.click_retention_days);
    println!("========================================");

    let bind_host = config.host.clone();
    let bind_port = config.port;

    // Initialize database connection
    let app_state = web::Data::new(
        AppState::new(config)
            .expect("Failed to connect to database. Make sure the SQLite container is running and ./data/rus.db exists.")
    );

    println!("✓ Database connection established");
    println!("🚀 Starting server on {}:{}", bind_host, bind_port);

    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(jwt_validator);
        let admin_auth = HttpAuthentication::bearer(admin_validator);

        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            // Public API routes - MUST BE BEFORE scoped /api routes
            .route("/api/register", web::post().to(register))
            .route("/api/login", web::post().to(login))
            .route("/api/refresh", web::post().to(refresh_token))
            .route("/api/config", web::get().to(get_config))
            .route("/api/setup/required", web::get().to(check_setup_required))
            // Admin-only routes - MUST BE BEFORE /api scope
            .service(
                web::scope("/api/admin")
                    .wrap(admin_auth)
                    .route("/users", web::get().to(admin_list_users))
                    .route("/users/{user_id}", web::delete().to(admin_delete_user))
                    .route("/stats", web::get().to(admin_get_stats))
            )
            // Protected routes (require authentication)
            .service(
                web::scope("/api")
                    .wrap(auth)
                    .route("/me", web::get().to(get_current_user))
                    .route("/shorten", web::post().to(shorten_url))
                    .route("/stats/{code}", web::get().to(get_stats))
                    .route("/urls", web::get().to(get_user_urls))
                    .route("/urls/{code}", web::delete().to(delete_url))
                    .route("/urls/{code}/name", web::patch().to(update_url_name))
                    .route("/urls/{code}/clicks", web::get().to(get_click_history))
                    .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code))
            )
            // Public page routes
            .route("/", web::get().to(index))
            .route("/login.html", web::get().to(login_page))
            .route("/signup.html", web::get().to(signup_page))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/setup.html", web::get().to(setup_page))
            .route("/admin.html", web::get().to(admin_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/auth.js", web::get().to(serve_auth_js))
            .route("/health", web::get().to(health_check))
            // Catch-all route for short code redirects (MUST BE LAST)
            .route("/{code}", web::get().to(redirect_url))
    })
    .bind((bind_host.as_str(), bind_port))?
    .run()
    .await
}
