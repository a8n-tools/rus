use actix_web::{web, App, HttpResponse, HttpServer, Result, middleware};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use rand::Rng;

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
    clicks: u64,
}

// Application state
struct AppState {
    urls: Mutex<HashMap<String, UrlEntry>>,
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

// API endpoint to shorten a URL
async fn shorten_url(
    data: web::Data<AppState>,
    req: web::Json<ShortenRequest>,
) -> Result<HttpResponse> {
    let mut urls = data.urls.lock().unwrap();
    
    // Validate URL
    if req.url.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "URL cannot be empty"
        })));
    }
    
    // Check if URL is already shortened
    for (code, entry) in urls.iter() {
        if entry.original_url == req.url {
            return Ok(HttpResponse::Ok().json(ShortenResponse {
                short_code: code.clone(),
                short_url: format!("http://localhost:8080/{}", code),
                original_url: entry.original_url.clone(),
            }));
        }
    }
    
    // Generate a unique short code
    let mut short_code = generate_short_code();
    while urls.contains_key(&short_code) {
        short_code = generate_short_code();
    }
    
    // Store the URL
    let entry = UrlEntry {
        original_url: req.url.clone(),
        short_code: short_code.clone(),
        clicks: 0,
    };
    
    urls.insert(short_code.clone(), entry);
    
    Ok(HttpResponse::Ok().json(ShortenResponse {
        short_code: short_code.clone(),
        short_url: format!("http://localhost:8080/{}", short_code),
        original_url: req.url.clone(),
    }))
}

// Endpoint to redirect to the original URL
async fn redirect_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let mut urls = data.urls.lock().unwrap();
    
    if let Some(entry) = urls.get_mut(code.as_str()) {
        entry.clicks += 1;
        let original_url = entry.original_url.clone();
        Ok(HttpResponse::Found()
            .append_header(("Location", original_url))
            .finish())
    } else {
        Ok(HttpResponse::NotFound().body("Short URL not found"))
    }
}

// API endpoint to get URL statistics
async fn get_stats(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let urls = data.urls.lock().unwrap();
    
    if let Some(entry) = urls.get(code.as_str()) {
        Ok(HttpResponse::Ok().json(entry))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found"
        })))
    }
}

// Serve the main HTML page
async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html")))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("🚀 Starting Rust URL Shortener on http://localhost:8080");
    
    let app_state = web::Data::new(AppState {
        urls: Mutex::new(HashMap::new()),
    });
    
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            .route("/", web::get().to(index))
            .route("/api/shorten", web::post().to(shorten_url))
            .route("/api/stats/{code}", web::get().to(get_stats))
            .route("/{code}", web::get().to(redirect_url))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
