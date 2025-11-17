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

const TOKEN_EXPIRY_HOURS: i64 = 24;
const HOST: &str = "0.0.0.0";

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
    username: String,
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

// Application state
struct AppState {
    db: Mutex<Connection>,
}

impl AppState {
    fn new(db_path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        Ok(AppState {
            db: Mutex::new(conn),
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

// Helper function to get JWT secret from environment
fn get_jwt_secret() -> String {
    env::var("JWT_SECRET").unwrap_or_else(|_| {
        eprintln!("WARNING: JWT_SECRET not set in environment, using default (insecure)");
        "your-secret-key-change-this-in-production".to_string()
    })
}

// JWT helper functions
fn create_jwt(username: &str, user_id: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(TOKEN_EXPIRY_HOURS))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        user_id,
        exp: expiration as usize,
    };

    let secret = get_jwt_secret();
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
}

fn decode_jwt(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let secret = get_jwt_secret();
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

    match decode_jwt(token) {
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
    match db.execute(
        "INSERT INTO users (username, password) VALUES (?1, ?2)",
        params![&req.username, &hashed_password],
    ) {
        Ok(_) => {
            // Get the user ID
            let user_id: i64 = db.last_insert_rowid();

            // Create JWT token
            match create_jwt(&req.username, user_id) {
                Ok(token) => Ok(HttpResponse::Created().json(AuthResponse {
                    token,
                    username: req.username.clone(),
                })),
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
            // Verify password
            match verify(&req.password, &hashed_password) {
                Ok(true) => {
                    // Create JWT token
                    match create_jwt(&username, user_id) {
                        Ok(token) => Ok(HttpResponse::Ok().json(AuthResponse {
                            token,
                            username,
                        })),
                        Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": "Failed to create token"
                        }))),
                    }
                }
                Ok(false) => Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "Invalid credentials"
                }))),
                Err(_) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "Authentication error"
                }))),
            }
        }
        Err(_) => Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid credentials"
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
            short_url: format!("http://localhost:8080/{}", short_code),
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
            short_url: format!("http://localhost:8080/{}", short_code),
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

    // Get URL and increment clicks
    let result: rusqlite::Result<String> = db.query_row(
        "SELECT original_url FROM urls WHERE short_code = ?1",
        params![code.as_str()],
        |row| row.get(0),
    );

    match result {
        Ok(original_url) => {
            // Increment click count
            let _ = db.execute(
                "UPDATE urls SET clicks = clicks + 1 WHERE short_code = ?1",
                params![code.as_str()],
            );

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
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    println!("🚀 Starting Rust URL Shortener with Authentication on {}:8080", HOST);

    // Initialize database connection
    let db_path = "./data/rus.db";
    let app_state = web::Data::new(
        AppState::new(db_path)
            .expect("Failed to connect to database. Make sure the SQLite container is running and ./data/rus.db exists.")
    );

    println!("✓ Connected to database at {}", db_path);

    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(jwt_validator);

        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            // Public routes
            .route("/", web::get().to(index))
            .route("/login.html", web::get().to(login_page))
            .route("/signup.html", web::get().to(signup_page))
            .route("/dashboard.html", web::get().to(dashboard_page))
            .route("/styles.css", web::get().to(serve_css))
            .route("/auth.js", web::get().to(serve_auth_js))
            .route("/api/register", web::post().to(register))
            .route("/api/login", web::post().to(login))
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
            )
    })
    .bind((HOST, 8080))?
    .run()
    .await
}
