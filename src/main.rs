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
use qrcode::QrCode;
use qrcode::render::svg;
use image::{Luma, DynamicImage, ImageBuffer, Rgba, imageops};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use url::Url;
use regex::Regex;

// Configuration with defaults
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
        Config {
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
                eprintln!("WARNING: JWT_SECRET not set, using insecure default");
                "your-secret-key-change-this-in-production".to_string()
            }),
            jwt_expiry_hours: env::var("JWT_EXPIRY")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            refresh_token_expiry_days: env::var("REFRESH_TOKEN_EXPIRY")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .unwrap_or(7),
            max_url_length: env::var("MAX_URL_LENGTH")
                .unwrap_or_else(|_| "2048".to_string())
                .parse()
                .unwrap_or(2048),
            account_lockout_attempts: env::var("ACCOUNT_LOCKOUT_ATTEMPTS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            account_lockout_duration_minutes: env::var("ACCOUNT_LOCKOUT_DURATION")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            click_retention_days: env::var("CLICK_RETENTION_DAYS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            host_url: env::var("HOST_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            db_path: env::var("DB_PATH")
                .unwrap_or_else(|_| "./data/rus.db".to_string()),
            host: env::var("HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
        }
    }
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
    created_at: String,
}

#[derive(Serialize)]
struct ClickHistoryEntry {
    clicked_at: String,
}

#[derive(Serialize)]
struct ClickStats {
    total_clicks: u64,
    history: Vec<ClickHistoryEntry>,
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
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct RefreshResponse {
    token: String,
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateUrlNameRequest {
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,      // username
    user_id: i64,     // user ID
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

// Application state
struct AppState {
    db: Mutex<Connection>,
    config: Config,
    start_time: std::time::Instant,
}

impl AppState {
    fn new(config: Config) -> rusqlite::Result<Self> {
        let conn = Connection::open(&config.db_path)?;

        // Initialize tables if they don't exist
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                userID INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password TEXT NOT NULL,
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
            CREATE INDEX IF NOT EXISTS idx_login_attempts_attempted_at ON login_attempts(attempted_at);"
        )?;

        Ok(AppState {
            db: Mutex::new(conn),
            config,
            start_time: std::time::Instant::now(),
        })
    }
}

// Password validation
fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters long".to_string());
    }

    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_number = password.chars().any(|c| c.is_numeric());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    if !has_uppercase {
        return Err("Password must contain at least one uppercase letter".to_string());
    }
    if !has_number {
        return Err("Password must contain at least one number".to_string());
    }
    if !has_special {
        return Err("Password must contain at least one special character".to_string());
    }

    Ok(())
}

// URL validation
fn validate_url(url_str: &str, max_length: usize) -> Result<(), String> {
    if url_str.len() > max_length {
        return Err(format!("URL exceeds maximum length of {} characters", max_length));
    }

    let parsed = Url::parse(url_str).map_err(|_| "Invalid URL format")?;

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

// Generate refresh token
fn generate_refresh_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    BASE64.encode(&bytes)
}

// JWT helper functions
fn create_jwt(username: &str, user_id: i64, secret: &str, expiry_hours: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(expiry_hours))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        user_id,
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

    let app_state = req.app_data::<web::Data<AppState>>()
        .expect("AppState not found");

    match decode_jwt(token, &app_state.config.jwt_secret) {
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

// Extract claims from request
fn get_claims(req: &HttpRequest) -> Option<Claims> {
    req.extensions().get::<Claims>().cloned()
}

// Check account lockout
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

// Record login attempt
fn record_login_attempt(db: &Connection, username: &str, success: bool) {
    let _ = db.execute(
        "INSERT INTO login_attempts (username, success) VALUES (?1, ?2)",
        params![username, success as i32],
    );
}

// Clean old click history
fn cleanup_old_clicks(db: &Connection, retention_days: i64) {
    let cutoff = Utc::now() - Duration::days(retention_days);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let _ = db.execute(
        "DELETE FROM click_history WHERE clicked_at < ?1",
        params![cutoff_str],
    );
}

// Generate QR code with Rust logo
fn generate_qr_code_png(url: &str) -> Result<Vec<u8>, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let image = code.render::<Luma<u8>>()
        .min_dimensions(400, 400)
        .build();

    // Convert to RGBA for logo overlay
    let mut rgba_image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        let luma = pixel.0[0];
        rgba_image.put_pixel(x, y, Rgba([luma, luma, luma, 255]));
    }

    // Create Rust logo (gear with R)
    let logo_size = image.width() / 5;
    let logo_x = (image.width() - logo_size) / 2;
    let logo_y = (image.height() - logo_size) / 2;

    // Draw orange circle for logo background
    let center_x = logo_x + logo_size / 2;
    let center_y = logo_y + logo_size / 2;
    let radius = logo_size / 2;

    for y in logo_y..(logo_y + logo_size) {
        for x in logo_x..(logo_x + logo_size) {
            let dx = x as i32 - center_x as i32;
            let dy = y as i32 - center_y as i32;
            if dx * dx + dy * dy <= (radius as i32 * radius as i32) {
                // Rust orange color: #CE422B -> RGB(206, 66, 43)
                rgba_image.put_pixel(x, y, Rgba([206, 66, 43, 255]));
            }
        }
    }

    // Draw "R" in white
    let r_size = logo_size / 2;
    let r_x = center_x - r_size / 3;
    let r_y = center_y - r_size / 2;
    let stroke_width = r_size / 6;

    // Vertical line of R
    for y in r_y..(r_y + r_size) {
        for x in r_x..(r_x + stroke_width) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Top horizontal of R
    for y in r_y..(r_y + stroke_width) {
        for x in r_x..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Middle horizontal of R
    for y in (r_y + r_size / 2 - stroke_width / 2)..(r_y + r_size / 2 + stroke_width / 2) {
        for x in r_x..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Right vertical of R (top half)
    for y in r_y..(r_y + r_size / 2) {
        for x in (r_x + r_size * 2 / 3 - stroke_width)..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Diagonal leg of R
    let leg_start_x = r_x + r_size / 3;
    let leg_start_y = r_y + r_size / 2;
    for i in 0..(r_size / 2) {
        let x = leg_start_x + i;
        let y = leg_start_y + i;
        for dx in 0..stroke_width {
            if x + dx < image.width() && y < image.height() {
                rgba_image.put_pixel(x + dx, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Convert to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    let dynamic_image = DynamicImage::ImageRgba8(rgba_image);
    dynamic_image.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}

fn generate_qr_code_svg(url: &str) -> Result<String, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let svg_string = code.render()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    // Insert Rust logo into SVG
    let logo_svg = r#"
    <circle cx="50%" cy="50%" r="10%" fill="#CE422B"/>
    <text x="50%" y="50%" text-anchor="middle" dominant-baseline="central"
          font-family="sans-serif" font-weight="bold" font-size="40" fill="white">R</text>
    "#;

    // Insert logo before closing </svg>
    let svg_with_logo = svg_string.replace("</svg>", &format!("{}</svg>", logo_svg));

    Ok(svg_with_logo)
}

// Health check endpoint
async fn health_check(data: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = data.start_time.elapsed().as_secs();

    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    }))
}

// Config endpoint (public)
async fn get_config(data: web::Data<AppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ConfigResponse {
        host_url: data.config.host_url.clone(),
        max_url_length: data.config.max_url_length,
    }))
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
    if let Err(e) = validate_password(&req.password) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": e
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
    match db.execute(
        "INSERT INTO users (username, password) VALUES (?1, ?2)",
        params![&req.username, &hashed_password],
    ) {
        Ok(_) => {
            let user_id: i64 = db.last_insert_rowid();

            // Create JWT token
            let token = match create_jwt(&req.username, user_id, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                Ok(t) => t,
                Err(_) => {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create token"
                    })));
                }
            };

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

    // Check for account lockout
    if is_account_locked(&db, &req.username, data.config.account_lockout_attempts, data.config.account_lockout_duration_minutes) {
        return Ok(HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": format!("Account locked due to too many failed attempts. Try again in {} minutes.", data.config.account_lockout_duration_minutes)
        })));
    }

    // Get user from database
    let mut stmt = match db.prepare("SELECT userID, username, password FROM users WHERE username = ?1") {
        Ok(stmt) => stmt,
        Err(_) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Database error"
            })));
        }
    };

    let user_result: rusqlite::Result<(i64, String, String)> = stmt.query_row(
        params![&req.username],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match user_result {
        Ok((user_id, username, hashed_password)) => {
            match verify(&req.password, &hashed_password) {
                Ok(true) => {
                    // Record successful login
                    record_login_attempt(&db, &req.username, true);

                    // Create JWT token
                    let token = match create_jwt(&username, user_id, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                        Ok(t) => t,
                        Err(_) => {
                            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                                "error": "Failed to create token"
                            })));
                        }
                    };

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
                Ok(false) => {
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
            record_login_attempt(&db, &req.username, false);
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Invalid credentials"
            })))
        }
    }
}

async fn refresh_token(
    data: web::Data<AppState>,
    req: web::Json<RefreshRequest>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    // Find and validate refresh token
    let token_result: rusqlite::Result<(i64, i64, String)> = db.query_row(
        "SELECT rt.id, rt.user_id, u.username FROM refresh_tokens rt
         JOIN users u ON rt.user_id = u.userID
         WHERE rt.token = ?1 AND rt.expires_at > datetime('now')",
        params![&req.refresh_token],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match token_result {
        Ok((token_id, user_id, username)) => {
            // Delete old refresh token
            let _ = db.execute("DELETE FROM refresh_tokens WHERE id = ?1", params![token_id]);

            // Create new JWT token
            let token = match create_jwt(&username, user_id, &data.config.jwt_secret, data.config.jwt_expiry_hours) {
                Ok(t) => t,
                Err(_) => {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to create token"
                    })));
                }
            };

            // Create new refresh token
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

    if let Err(e) = validate_url(&req_payload.url, data.config.max_url_length) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": e
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
            // Increment click count
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
        Err(_) => Ok(HttpResponse::NotFound().body("Short URL not found")),
    }
}

// Protected API endpoint to get URL statistics
async fn get_stats(
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

    let result: rusqlite::Result<UrlEntry> = db.query_row(
        "SELECT original_url, short_code, name, clicks, created_at FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
        |row| {
            Ok(UrlEntry {
                original_url: row.get(0)?,
                short_code: row.get(1)?,
                name: row.get(2)?,
                clicks: row.get(3)?,
                created_at: row.get(4)?,
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

// Get click history for a URL
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

    // Get total clicks
    let total_clicks: u64 = db.query_row(
        "SELECT clicks FROM urls WHERE id = ?1",
        params![url_id],
        |row| row.get(0),
    ).unwrap_or(0);

    // Get click history
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

// Generate QR code for a URL
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

    let full_url = format!("{}/{}", data.config.host_url, code);
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
        "SELECT original_url, short_code, name, clicks, created_at FROM urls WHERE user_id = ?1 ORDER BY created_at DESC"
    ).map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let urls: Vec<UrlEntry> = stmt.query_map(params![claims.user_id], |row| {
        Ok(UrlEntry {
            original_url: row.get(0)?,
            short_code: row.get(1)?,
            name: row.get(2)?,
            clicks: row.get(3)?,
            created_at: row.get(4)?,
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env();
    let host = config.host.clone();
    let port = config.port;
    let db_path = config.db_path.clone();

    println!("🦀 Starting Rust URL Shortener v{}", env!("CARGO_PKG_VERSION"));
    println!("📍 Host URL: {}", config.host_url);
    println!("🔒 JWT Expiry: {} hour(s)", config.jwt_expiry_hours);
    println!("🔄 Refresh Token Expiry: {} day(s)", config.refresh_token_expiry_days);
    println!("📏 Max URL Length: {}", config.max_url_length);
    println!("🚫 Account Lockout: {} attempts / {} minutes", config.account_lockout_attempts, config.account_lockout_duration_minutes);
    println!("📊 Click Retention: {} days", config.click_retention_days);

    let app_state = web::Data::new(
        AppState::new(config)
            .expect("Failed to connect to database")
    );

    println!("✓ Connected to database at {}", db_path);
    println!("🚀 Server starting on {}:{}", host, port);

    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(jwt_validator);

        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            // Public routes
            .route("/", web::get().to(index))
            .route("/health", web::get().to(health_check))
            .route("/api/config", web::get().to(get_config))
            .route("/login.html", web::get().to(login_page))
            .route("/signup.html", web::get().to(signup_page))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/auth.js", web::get().to(serve_auth_js))
            .route("/api/register", web::post().to(register))
            .route("/api/login", web::post().to(login))
            .route("/api/refresh", web::post().to(refresh_token))
            .route("/{code}", web::get().to(redirect_url))
            // Protected routes (require authentication)
            .service(
                web::scope("/api")
                    .wrap(auth)
                    .route("/shorten", web::post().to(shorten_url))
                    .route("/stats/{code}", web::get().to(get_stats))
                    .route("/urls", web::get().to(get_user_urls))
                    .route("/urls/{code}", web::delete().to(delete_url))
                    .route("/urls/{code}/name", web::patch().to(update_url_name))
                    .route("/urls/{code}/clicks", web::get().to(get_click_history))
                    .route("/urls/{code}/qr/{format}", web::get().to(get_qr_code))
            )
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
