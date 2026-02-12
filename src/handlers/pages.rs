use actix_web::{web, HttpResponse, Result};
#[cfg(feature = "saas")]
use actix_web::HttpRequest;
#[cfg(feature = "saas")]
use base64::Engine;

use crate::db::AppState;
#[cfg(feature = "standalone")]
use crate::models::SetupCheckResponse;
use crate::models::{ConfigResponse, HealthResponse, VersionResponse};

/// Serve static HTML pages
pub async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/index.html")))
}

#[cfg(feature = "standalone")]
pub async fn login_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/login.html")))
}

#[cfg(feature = "standalone")]
pub async fn signup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/signup.html")))
}

/// Dashboard page for standalone mode - no cookie check needed
#[cfg(feature = "standalone")]
pub async fn dashboard_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/dashboard.html")))
}

/// Dashboard page for SaaS mode - checks access_token cookie from parent app
#[cfg(feature = "saas")]
pub async fn dashboard_page(req: HttpRequest) -> Result<HttpResponse> {
    let redirect_url = "https://app.a8n.run";

    // Check for access_token cookie from parent app
    let should_redirect = match req.cookie("access_token") {
        None => true,
        Some(cookie) => {
            let parts: Vec<&str> = cookie.value().split('.').collect();
            if parts.len() != 3 {
                true
            } else {
                match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
                    Ok(bytes) => {
                        match serde_json::from_slice::<serde_json::Value>(&bytes) {
                            Ok(payload) => {
                                payload.get("membership_status")
                                    .and_then(|v| v.as_str())
                                    == Some("canceled")
                            }
                            Err(_) => true,
                        }
                    }
                    // Try standard base64 as fallback
                    Err(_) => match base64::engine::general_purpose::STANDARD.decode(parts[1]) {
                        Ok(bytes) => {
                            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                                Ok(payload) => {
                                    payload.get("membership_status")
                                        .and_then(|v| v.as_str())
                                        == Some("canceled")
                                }
                                Err(_) => true,
                            }
                        }
                        Err(_) => true,
                    },
                }
            }
        }
    };

    if should_redirect {
        return Ok(HttpResponse::Found()
            .append_header(("Location", redirect_url))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/dashboard.html")))
}

#[cfg(feature = "standalone")]
pub async fn setup_page() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/setup.html")))
}

#[cfg(feature = "standalone")]
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

#[cfg(feature = "standalone")]
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

/// Check if initial setup is required (no users exist) - standalone only
#[cfg(feature = "standalone")]
pub async fn check_setup_required(data: web::Data<AppState>) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    let user_count: i64 = db
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(SetupCheckResponse {
        setup_required: user_count == 0,
    }))
}

/// Get application version
pub async fn get_version() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
