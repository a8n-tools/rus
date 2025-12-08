use actix_web::{web, HttpResponse, Result};

use crate::db::AppState;
use crate::models::{ConfigResponse, HealthResponse, SetupCheckResponse};

/// Serve static HTML pages
pub async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/index.html")))
}

pub async fn login_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/login.html")))
}

pub async fn signup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/signup.html")))
}

pub async fn dashboard_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/dashboard.html")))
}

pub async fn setup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/setup.html")))
}

pub async fn admin_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/admin.html")))
}

pub async fn serve_css() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/styles.css")))
}

pub async fn serve_auth_js() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/javascript; charset=utf-8")
        .body(include_str!("../../static/auth.js")))
}

/// Health check endpoint for monitoring and Docker health checks
pub async fn health_check(data: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = data.start_time.elapsed().as_secs();

    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    }))
}

/// Public config endpoint for frontend
pub async fn get_config(data: web::Data<AppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ConfigResponse {
        host_url: data.config.host_url.clone(),
        max_url_length: data.config.max_url_length,
        allow_registration: data.config.allow_registration,
    }))
}

/// Check if initial setup is required (no users exist)
pub async fn check_setup_required(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let user_count: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(SetupCheckResponse {
        setup_required: user_count == 0,
    }))
}
